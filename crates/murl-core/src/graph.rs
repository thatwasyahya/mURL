//! Dependency ordering over the resources of a single manifest.
//!
//! Resources within one manifest form a DAG through `dependsOn` edges.
//! Version 0.1 assigns `dependsOn` exactly one runtime meaning: *launch
//! ordering* — a resource is dispatched after everything it depends on.
//! `relations` edges are pure metadata in v0.1 and take no part in ordering
//! (see `docs/architecture.md` §"Graph, deliberately small").
//!
//! Cycles are validation errors, detected here with Kahn's algorithm. Ties
//! are broken by (`order`, declaration index), which makes plans stable
//! across runs — an auditability property, not an aesthetic one.

use crate::manifest::ResourceDoc;

/// Compute the dispatch order of resource indices, honoring `dependsOn`
/// edges and breaking ties by (`order`, index). Returns the cycle path on
/// failure.
pub fn execution_order(resources: &[ResourceDoc]) -> Result<Vec<usize>, String> {
    let n = resources.len();
    let index_of = |id: &str| resources.iter().position(|r| r.id == id);

    // dependency count per node; edges validated upstream but tolerated here.
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, r) in resources.iter().enumerate() {
        for d in &r.depends_on {
            if let Some(j) = index_of(d) {
                if i != j {
                    deps[i].push(j);
                }
            }
        }
    }

    let mut done = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        // Smallest (order, index) among nodes whose dependencies are done.
        let mut pick: Option<usize> = None;
        for i in 0..n {
            if done[i] || !deps[i].iter().all(|&j| done[j]) {
                continue;
            }
            match pick {
                None => pick = Some(i),
                Some(p) => {
                    if (resources[i].order, i) < (resources[p].order, p) {
                        pick = Some(i);
                    }
                }
            }
        }
        match pick {
            Some(i) => {
                done[i] = true;
                out.push(i);
            }
            None => {
                let stuck: Vec<&str> = resources
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !done[*i])
                    .map(|(_, r)| r.id.as_str())
                    .collect();
                return Err(format!(
                    "dependsOn cycle among resources: {}",
                    stuck.join(" -> ")
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ResourceDoc;

    fn res(id: &str, order: i64, deps: &[&str]) -> ResourceDoc {
        ResourceDoc {
            id: id.into(),
            kind: "https".into(),
            target: format!("https://example.com/{id}"),
            label: None,
            role: None,
            required: false,
            order,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            tags: Vec::new(),
            integrity: None,
            meta: None,
        }
    }

    #[test]
    fn orders_by_order_field_then_index() {
        let rs = vec![res("c", 30, &[]), res("a", 10, &[]), res("b", 10, &[])];
        let order = execution_order(&rs).unwrap();
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn honors_dependencies_over_order() {
        // `first` has the highest order value but everything depends on it.
        let rs = vec![
            res("x", 1, &["first"]),
            res("y", 2, &["first"]),
            res("first", 99, &[]),
        ];
        let order = execution_order(&rs).unwrap();
        assert_eq!(order, vec![2, 0, 1]);
    }

    #[test]
    fn detects_cycles() {
        let rs = vec![res("a", 1, &["b"]), res("b", 1, &["a"])];
        let err = execution_order(&rs).unwrap_err();
        assert!(err.contains("cycle"));
    }

    #[test]
    fn detects_self_reference_via_validation_but_tolerates_here() {
        // Self-edges are stripped (validator rejects them separately).
        let rs = vec![res("a", 1, &["a"])];
        assert_eq!(execution_order(&rs).unwrap(), vec![0]);
    }

    #[test]
    fn empty_is_fine() {
        assert!(execution_order(&[]).unwrap().is_empty());
    }
}
