//! Opt-in parallelism: splits the planned suites into independent workers (`qaspec run --jobs N`).
//!
//! Each worker gets its own agent-browser session (its own Chromium), its own LLM client and its own
//! thread, so suites must not share state that only one browser can hold: signed-in identities,
//! `depends_on` ordering, cross-project flows and `${capture}` values. The partitioner therefore
//! computes the transitive closure of those links first (they become atomic "chunks"), and only then
//! spreads the chunks over the workers, balanced by number of steps.

use crate::config::Config;
use crate::runner::Planned;
use crate::spec::{CaptureSource, Check, Item};
use anyhow::{bail, Result};
use regex::Regex;
use std::collections::{BTreeSet, HashMap};

/// One unit of work that cannot be split across workers.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Suite indices in plan order.
    pub suites: Vec<usize>,
    /// Total number of steps.
    pub steps: usize,
}

impl Chunk {
    pub fn first(&self) -> usize {
        self.suites[0]
    }
}

/// Splits `planned` into `jobs` groups that can run independently, each one keeping plan order.
///
/// Groups are returned in ascending plan order of their first suite. Device suites (a suite with a
/// `device`) all go to the **first** worker and stay last in it: agent-browser applies `set device`
/// to the whole browser and cannot undo the user agent, so two workers could each run one safely
/// but a desktop suite after a device suite in the same worker could not.
pub fn partition(cfg: &Config, planned: &[Planned], jobs: usize) -> Result<Vec<Vec<usize>>> {
    if jobs <= 1 {
        return Ok(vec![(0..planned.len()).collect()]);
    }
    if planned.is_empty() {
        return Ok(Vec::new());
    }
    let links = links(cfg, planned);
    let chunks = chunks(planned, &links);
    let n = jobs.min(chunks.len());
    let groups = balance(chunks, n);
    let mut groups: Vec<Vec<usize>> = groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|mut g| {
            g.sort_unstable();
            g
        })
        .collect();
    // A device suite poisons the user agent of its whole worker: put them all together, and last.
    let devices: Vec<usize> = planned
        .iter()
        .enumerate()
        .filter(|(_, p)| crate::runner::is_device_suite(cfg, p))
        .map(|(i, _)| i)
        .collect();
    if !devices.is_empty() && !groups.is_empty() {
        // Take them out of every worker first, then append them all to the first one.
        for g in groups.iter_mut().skip(1) {
            g.retain(|j| !devices.contains(j));
        }
        groups[0].retain(|j| !devices.contains(j));
        groups[0].extend(devices);
        groups[0].sort_unstable();
        groups.retain(|g| !g.is_empty());
    }
    Ok(groups)
}

/// The suites that must stay together.
///
/// Three things tie two suites to the same worker:
/// 1. Same `(project, identity)`: one browser session holds one signed-in identity at a time, and
///    the state file of an identity is written by one worker only.
/// 2. Projects connected by `depends_on` or by a step that switches `project:`: the identities they
///    share are kept in the same worker, in plan order, so the dependency is signed in before its
///    dependents need it and a cross-project flow keeps its tabs together.
/// 3. A `${capture}` produced by one suite and used by a later one: the value only exists in the
///    worker that captured it.
///
/// Suites without an identity have no state to share, so they only get linked by (3).
pub fn links(cfg: &Config, planned: &[Planned]) -> Vec<Vec<usize>> {
    let mut uf = UnionFind::new(planned.len());
    // 1. Same (project, identity).
    let mut by_project_identity: HashMap<(String, String), usize> = HashMap::new();
    for (i, p) in planned.iter().enumerate() {
        let Some(id) = &p.suite.identity else {
            continue;
        };
        let key = (p.project.clone(), id.clone());
        match by_project_identity.get(&key) {
            Some(j) => uf.union(*j, i),
            None => {
                by_project_identity.insert(key, i);
            }
        }
    }
    // 2. Connected projects share their identities.
    let mut comp = project_components(cfg, planned);
    let mut by_component_identity: HashMap<(usize, String), usize> = HashMap::new();
    for (i, p) in planned.iter().enumerate() {
        let Some(id) = &p.suite.identity else {
            continue;
        };
        let key = (component_of(&mut comp, &p.project), id.clone());
        match by_component_identity.get(&key) {
            Some(j) => uf.union(*j, i),
            None => {
                by_component_identity.insert(key, i);
            }
        }
        // A step that signs in as another identity of another project joins that thread too.
        for st in &p.suite.steps {
            let (Some(pr), Some(as_id)) = (st.project.as_ref(), st.identity.as_ref()) else {
                continue;
            };
            let same = planned.iter().position(|q| {
                q.project == *pr && q.suite.identity.as_deref() == Some(as_id.as_str())
            });
            if let Some(j) = same {
                uf.union(i, j);
            }
        }
    }
    // 3. Captures used by a later suite.
    let captures: Vec<(usize, String)> = planned
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            p.suite.steps.iter().flat_map(move |st| {
                st.items.iter().filter_map(move |it| match it {
                    Item::Capture { name, .. } => Some((i, name.clone())),
                    _ => None,
                })
            })
        })
        .collect();
    for (i, p) in planned.iter().enumerate() {
        for name in referenced_captures(&p.suite) {
            for (j, c) in &captures {
                if *j <= i && *c == name {
                    uf.union(i, *j);
                }
            }
        }
    }
    uf.groups()
}

/// Projects connected by `depends_on` or by a step that switches `project:`, numbered by component.
fn project_components(cfg: &Config, planned: &[Planned]) -> HashMap<String, usize> {
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (name, p) in &cfg.projects {
        for d in &p.depends_on {
            edges.entry(name.clone()).or_default().insert(d.clone());
            edges.entry(d.clone()).or_default().insert(name.clone());
        }
    }
    for p in planned {
        for st in p.suite.steps.iter().filter_map(|s| s.project.clone()) {
            edges
                .entry(p.project.clone())
                .or_default()
                .insert(st.clone());
            edges.entry(st).or_default().insert(p.project.clone());
        }
    }
    let mut comp: HashMap<String, usize> = HashMap::new();
    let mut names: Vec<String> = edges.keys().cloned().collect();
    names.sort();
    let mut next = 0;
    for start in names {
        if comp.contains_key(&start) {
            continue;
        }
        let mut stack = vec![start];
        while let Some(n) = stack.pop() {
            if comp.contains_key(&n) {
                continue;
            }
            comp.insert(n.clone(), next);
            if let Some(ns) = edges.get(&n) {
                stack.extend(ns.iter().cloned());
            }
        }
        next += 1;
    }
    comp
}

/// Normalizes project names to their component id, adding unseen projects as their own component.
fn component_of(comp: &mut HashMap<String, usize>, project: &str) -> usize {
    if let Some(c) = comp.get(project) {
        return *c;
    }
    let next = comp.len();
    comp.insert(project.to_string(), next);
    next
}

/// `${name}` placeholders inside a suite that are not config placeholders.
fn referenced_captures(suite: &crate::spec::Suite) -> Vec<String> {
    let re = Regex::new(r"\$\{\s*([A-Za-z0-9_.\-]+)\s*\}").unwrap();
    let reserved = |k: &str| {
        k.starts_with("params.")
            || k.starts_with("project.")
            || k.starts_with("projects.")
            || k.starts_with("identity.")
            || k.starts_with("run.")
            || k == "env"
    };
    let mut out = Vec::new();
    let mut push = |text: &str| {
        for c in re.captures_iter(text) {
            let k = &c[1];
            if !reserved(k) && !out.contains(&k.to_string()) {
                out.push(k.to_string());
            }
        }
    };
    for st in &suite.steps {
        if let Some(s) = &st.start {
            push(s);
        }
        for it in &st.items {
            match it {
                Item::Goal { text, .. } => push(text),
                Item::Capture {
                    source: CaptureSource::Describe { description },
                    ..
                } => push(description),
                Item::Expect { check, .. } => match check {
                    Check::Judge { text } => push(text),
                    Check::Url { value, .. } | Check::Visible { text: value, .. } => push(value),
                    Check::Network { pattern, .. } => push(pattern),
                    _ => {}
                },
                Item::Capture { .. } => {}
            }
        }
    }
    out
}

/// Connected components of the link graph, each with its suites in plan order and its step count.
pub fn chunks(planned: &[Planned], links: &[Vec<usize>]) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = links
        .iter()
        .map(|members| {
            let mut suites = members.clone();
            suites.sort_unstable();
            Chunk {
                steps: suites.iter().map(|i| planned[*i].suite.steps.len()).sum(),
                suites,
            }
        })
        .collect();
    out.sort_by_key(|c| c.first());
    out
}

/// Largest-first assignment of chunks to `jobs` buckets, so the heaviest suite starts first.
fn balance(chunks: Vec<Chunk>, jobs: usize) -> Vec<Vec<usize>> {
    let mut buckets: Vec<(usize, Vec<usize>)> = vec![(0, Vec::new()); jobs];
    let mut by_weight = chunks;
    by_weight.sort_by(|a, b| b.steps.cmp(&a.steps).then(a.first().cmp(&b.first())));
    for c in by_weight {
        let k = (0..jobs).min_by_key(|i| buckets[*i].0).unwrap();
        buckets[k].0 += c.steps;
        buckets[k].1.extend(c.suites);
    }
    buckets.into_iter().map(|(_, s)| s).collect()
}

// ---------------------------------------------------------------------------------------------
// Memory guard
// ---------------------------------------------------------------------------------------------

/// RAM one Chromium (one agent-browser session) needs, in MiB.
pub const SESSION_MIB: u64 = 1536;
/// RAM that must stay free on the machine, in MiB.
pub const RESERVE_MIB: u64 = 2048;

/// Reads `MemAvailable` from a `/proc/meminfo` body (in kB), in MiB.
pub fn mem_available_mib(meminfo: &str) -> Option<u64> {
    meminfo.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == "MemAvailable")
            .then(|| v.trim().trim_end_matches(" kB").trim().parse::<u64>().ok())
            .flatten()
            .map(|kb| kb / 1024)
    })
}

/// How many sessions fit in the available memory, keeping `RESERVE_MIB` free.
pub fn sessions_that_fit(available_mib: u64) -> usize {
    let usable = available_mib.saturating_sub(RESERVE_MIB);
    if usable < SESSION_MIB {
        0
    } else {
        (usable / SESSION_MIB) as usize
    }
}

/// Clamps `--jobs N` to what the machine can afford. Returns the effective number of jobs and a
/// warning to print (empty when nothing had to change).
pub fn guard(jobs: usize, force: bool) -> Result<(usize, String)> {
    if jobs <= 1 || force {
        return Ok((jobs, String::new()));
    }
    let meminfo = match std::fs::read_to_string("/proc/meminfo") {
        Ok(m) => m,
        Err(_) => {
            return Ok((
                jobs,
                format!(
                    "warning: cannot read /proc/meminfo, running {jobs} sessions without the memory guard"
                ),
            ))
        }
    };
    let available = match mem_available_mib(&meminfo) {
        Some(a) => a,
        None => {
            return Ok((
                jobs,
                format!(
                    "warning: /proc/meminfo has no MemAvailable, running {jobs} sessions without the memory guard"
                ),
            ))
        }
    };
    let fits = sessions_that_fit(available);
    if fits >= jobs {
        return Ok((jobs, String::new()));
    }
    let n = fits.max(1);
    if fits == 0 {
        return Ok((
            1,
            format!(
                "warning: only {available} MiB available, not enough for a second browser session \
                 ({SESSION_MIB} MiB each + {RESERVE_MIB} MiB in reserve): running 1 session. \
                 Use --force to override."
            ),
        ));
    }
    Ok((
        n,
        format!(
            "warning: only {available} MiB available, {jobs} sessions need {} MiB (+ {} MiB in reserve): \
             running {n} session(s). Use --force to override.",
            jobs as u64 * SESSION_MIB,
            RESERVE_MIB
        ),
    ))
}

// ---------------------------------------------------------------------------------------------
// union-find
// ---------------------------------------------------------------------------------------------

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            // Keep the earliest suite as the root so plan order survives.
            self.parent[b.max(a)] = a.min(b);
        }
    }

    /// The members of each set, keyed by root, in ascending order.
    fn groups(&self) -> Vec<Vec<usize>> {
        let mut out: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..self.parent.len() {
            let mut root = i;
            while self.parent[root] != root {
                root = self.parent[root];
            }
            out.entry(root).or_default().push(i);
        }
        out.into_values().collect()
    }
}

/// Workers must never write the same saved identity state file.
pub fn assert_disjoint_identities(
    cfg: &Config,
    groups: &[Vec<usize>],
    planned: &[Planned],
) -> Result<()> {
    let mut owner: HashMap<String, usize> = HashMap::new();
    for (w, g) in groups.iter().enumerate() {
        for i in g {
            let p = &planned[*i];
            let Some(id) = &p.suite.identity else {
                continue;
            };
            let key = cfg.state_dir().join(format!("{}.{}.json", p.project, id));
            let key = key.to_string_lossy().to_string();
            if let Some(prev) = owner.insert(key.clone(), w) {
                if prev != w {
                    bail!(
                        "internal error: workers {prev} and {w} would both write the state of \
                         {}.{} ({key})",
                        p.project,
                        id
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Overrides;
    use crate::spec;
    use std::path::PathBuf;

    fn cfg(toml: &str) -> Config {
        Config::from_str(toml, PathBuf::from("/root"), &Overrides::default()).unwrap()
    }

    fn planned_of(c: &Config, files: &[(&str, &str)]) -> Vec<Planned> {
        let specs: Vec<_> = files
            .iter()
            .map(|(p, s)| spec::parse(p, s).unwrap())
            .collect();
        crate::runner::plan(c, &specs).unwrap()
    }

    const TWO: &str = "[projects.api]\nbase_url='http://api.local'\nspecs='specs/api/*.qa.ts'\n                       [projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n";

    #[test]
    fn same_identity_stays_together() {
        let c = cfg(
            "[projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n\
             [projects.web.identities.qa]\nusername='a@b.c'\n\
             [projects.web.identities.ad]\nusername='a@b.c'\n",
        );
        let p = planned_of(
            &c,
            &[(
                "specs/web/a.qa.ts",
                "suite('w1', {as:'qa'}, () => { expect.errors.none() })\n\
                 suite('w2', {as:'ad'}, () => { expect.errors.none() })\n\
                 suite('w3', {as:'qa'}, () => { expect.errors.none() })\n\
                 suite('w4', () => { expect.errors.none() })",
            )],
        );
        // Plan order groups the identity: qa, qa, ad, anonymous.
        let names_all: Vec<&str> = p.iter().map(|x| x.suite.name.as_str()).collect();
        assert_eq!(names_all, vec!["w1", "w3", "w2", "w4"]);
        let g = partition(&c, &p, 4).unwrap();
        let qa = g.iter().find(|g| g.contains(&0)).unwrap();
        assert_eq!(*qa, vec![0, 1], "the two qa suites share a worker");
        assert!(g.iter().any(|g| *g == vec![2]), "admin has its own");
        assert!(g.iter().any(|g| *g == vec![3]), "anonymous has its own");
    }

    #[test]
    fn dependency_closure_keeps_the_shared_identity_together() {
        // `web` depends_on `api`: both are in the same project component, so the same identity
        // must be signed in once, in one worker, in plan order.
        let c = cfg(
            "[projects.api]\nbase_url='http://api.local'\nspecs='specs/api/*.qa.ts'\n\
             [projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\ndepends_on=['api']\n\
             [projects.web.identities.qa]\nusername='a@b.c'\n\
             [projects.api.identities.qa]\nusername='a@b.c'\n",
        );
        let p = planned_of(
            &c,
            &[
                (
                    "specs/api/a.qa.ts",
                    "suite('a1', {as:'qa'}, () => { expect.errors.none() })",
                ),
                (
                    "specs/web/b.qa.ts",
                    "suite('w1', {as:'qa'}, () => { expect.errors.none() })",
                ),
                (
                    "specs/api/c.qa.ts",
                    "suite('a2', () => { expect.errors.none() })",
                ),
            ],
        );
        let g = partition(&c, &p, 3).unwrap();
        let qa = p.iter().position(|x| x.suite.name == "w1").unwrap();
        let api = p.iter().position(|x| x.suite.name == "a1").unwrap();
        let anon = p.iter().position(|x| x.suite.name == "a2").unwrap();
        assert_eq!(
            *g.iter().find(|g| g.contains(&qa)).unwrap(),
            vec![api, qa],
            "the same identity in connected projects shares a worker"
        );
        assert_eq!(
            *g.iter().find(|g| g.contains(&anon)).unwrap(),
            vec![anon],
            "the anonymous suite is free"
        );
    }

    #[test]
    fn step_project_switch_shares_the_identity() {
        let c = cfg(
            "[projects.api]\nbase_url='http://api.local'\nspecs='specs/api/*.qa.ts'\n\
             [projects.api.identities.qa]\nusername='a@b.c'\n\
             [projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n",
        );
        let p = planned_of(
            &c,
            &[
                (
                    "specs/web/a.qa.ts",
                    "suite('cross', {project:'web', as:'qa'}, () => {\n\
                     \x20 step('go to api', {project:'api', as:'qa'}, () => { expect.errors.none() })\n\
                     \x20 step('back', {project:'web', as:'qa'}, () => { expect.errors.none() })\n\
                     })",
                ),
                (
                    "specs/api/b.qa.ts",
                    "suite('a1', {as:'qa'}, () => { expect.errors.none() })",
                ),
            ],
        );
        let g = partition(&c, &p, 3).unwrap();
        assert_eq!(
            g.len(),
            1,
            "a step that switches project+identity ties them: {g:?}"
        );
    }

    #[test]
    fn capture_link_pulls_later_suite() {
        // Like the fixture: `admin` runs after `app` (depends_on) and uses a capture from it.
        let c = cfg(
            "[projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n\
             [projects.api]\nbase_url='http://api.local'\nspecs='specs/api/*.qa.ts'\ndepends_on=['web']\n",
        );
        let p = planned_of(
            &c,
            &[
                (
                    "specs/web/a.qa.ts",
                    "suite('maker', () => { capture.state('n', 'window.n') })",
                ),
                (
                    "specs/web/b.qa.ts",
                    "suite('other', () => { expect.errors.none() })",
                ),
                (
                    "specs/api/c.qa.ts",
                    "suite('reader', () => { expect.visible('count ${n}') })",
                ),
            ],
        );
        let g = partition(&c, &p, 3).unwrap();
        let maker = p.iter().position(|x| x.suite.name == "maker").unwrap();
        let reader = p.iter().position(|x| x.suite.name == "reader").unwrap();
        let linked = g.iter().find(|g| g.contains(&maker)).unwrap();
        assert!(
            linked.contains(&reader),
            "the suite using the capture shares the worker"
        );
        let mut sorted = linked.clone();
        sorted.sort_unstable();
        assert_eq!(*linked, sorted, "plan order inside the worker");
    }

    #[test]
    fn balance_by_steps() {
        let c = cfg(TWO);
        let mut src = String::new();
        for (i, n) in [8usize, 5, 2, 2, 1].iter().enumerate() {
            let steps: Vec<String> = (0..*n)
                .map(|j| {
                    format!("step('s{i}_{j}', {{start:'/'}}, () => {{ expect.errors.none() }})")
                })
                .collect();
            src.push_str(&format!("suite('s{i}', () => {{ {} }})\n", steps.join(" ")));
        }
        let p = planned_of(&c, &[("specs/web/a.qa.ts", &src)]);
        let g = partition(&c, &p, 2).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.iter().map(|g| g.len()).collect::<Vec<_>>(), vec![2, 3]);
        // each worker keeps plan order
        for grp in &g {
            let mut s = grp.clone();
            s.sort_unstable();
            assert_eq!(*grp, s);
        }
    }

    #[test]
    fn jobs_one_is_the_whole_plan() {
        let c = cfg(TWO);
        let p = planned_of(
            &c,
            &[(
                "specs/web/a.qa.ts",
                "suite('a', () => { expect.errors.none() })",
            )],
        );
        assert_eq!(partition(&c, &p, 1).unwrap(), vec![vec![0]]);
    }

    #[test]
    fn saved_state_is_never_shared() {
        let c = cfg(
            "[projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n\
             [projects.web.identities.qa]\nusername='a@b.c'\n",
        );
        let p = planned_of(
            &c,
            &[(
                "specs/web/a.qa.ts",
                "suite('a', {as:'qa'}, () => { expect.errors.none() })",
            )],
        );
        assert!(assert_disjoint_identities(&c, &[vec![0]], &p).is_ok());
        let err = format!(
            "{:#}",
            assert_disjoint_identities(&c, &[vec![0], vec![0]], &p).unwrap_err()
        );
        assert!(err.contains("would both write"), "{err}");
    }

    #[test]
    fn device_suites_share_one_worker_and_run_last() {
        // agent-browser 0.27 cannot undo `set device`, so two workers could each run a device
        // suite, but a desktop suite after one in the SAME worker would get the phone user agent.
        // The partitioner therefore keeps every device suite together and last in the first
        // worker, whatever the balancing decided.
        let c = cfg(
            "[projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n\
             [projects.web.identities.one]\nusername='a@b.c'\n\
             [projects.web.identities.two]\nusername='d@e.f'\n\
             [projects.web.identities.three]\nusername='g@h.i'\n",
        );
        let p = planned_of(
            &c,
            &[
                (
                    "specs/web/a.qa.ts",
                    "suite('phone', {as:'one', device:'iPhone 14'}, () => { expect.errors.none() })",
                ),
                (
                    "specs/web/b.qa.ts",
                    "suite('desktop a', {as:'two'}, () => { expect.errors.none() })",
                ),
                (
                    "specs/web/c.qa.ts",
                    "suite('tablet', {as:'three', device:'iPad'}, () => { expect.errors.none() })",
                ),
                (
                    "specs/web/d.qa.ts",
                    "suite('desktop b', {as:'one'}, () => { expect.errors.none() })",
                ),
            ],
        );
        let g = partition(&c, &p, 4).unwrap();
        let devices: Vec<usize> = p
            .iter()
            .enumerate()
            .filter(|(_, x)| crate::runner::is_device_suite(&c, x))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(devices.len(), 2, "the plan has two device suites");
        for d in &devices {
            assert!(
                g[0].contains(d),
                "every device suite runs in worker 0: {g:?}"
            );
        }
        // Nothing but device suites runs after the first device suite of the worker.
        let first_device = *devices.iter().min().unwrap();
        for j in g[0].iter().filter(|j| **j > first_device) {
            assert!(devices.contains(j), "only device suites after it: {g:?}");
        }
        for other in g.iter().skip(1) {
            for j in other {
                assert!(!devices.contains(j), "no device suite outside worker 0");
            }
        }
    }

    #[test]
    fn a_viewport_only_suite_splits_freely() {
        // A `viewport` is restored by qaspec, so it is not a device suite and can go anywhere.
        let c = cfg(
            "[projects.web]\nbase_url='http://web.local'\nspecs='specs/web/*.qa.ts'\n\
             [projects.web.identities.one]\nusername='a@b.c'\n\
             [projects.web.identities.two]\nusername='d@e.f'\n",
        );
        let p = planned_of(
            &c,
            &[
                (
                    "specs/web/a.qa.ts",
                    "suite('small', {as:'one', viewport:[390,844]}, () => { expect.errors.none() })",
                ),
                (
                    "specs/web/b.qa.ts",
                    "suite('wide', {as:'two'}, () => { expect.errors.none() })",
                ),
            ],
        );
        assert!(!p.iter().any(|x| crate::runner::is_device_suite(&c, x)));
        let g = partition(&c, &p, 2).unwrap();
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn memory_guard_arithmetic() {
        let meminfo = "MemTotal:       64238988 kB\nMemFree:         100000 kB\nMemAvailable:   34183000 kB\n";
        assert_eq!(mem_available_mib(meminfo), Some(33381));
        // 33381 MiB - 2048 = 31333 / 1536 -> 20 sessions fit
        assert_eq!(sessions_that_fit(33381), 20);
        assert_eq!(sessions_that_fit(3000), 0);
        assert_eq!(sessions_that_fit(3584), 1);
        assert_eq!(mem_available_mib("MemTotal: 1 kB"), None);
    }
}
