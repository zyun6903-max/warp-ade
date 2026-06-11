pub fn dedupe_import_candidates<T, F, G>(
    mut items: Vec<T>,
    source_path: F,
    project_key: G,
    modified_at: fn(&T) -> i64,
) -> Vec<T>
where
    F: Fn(&T) -> String,
    G: Fn(&T) -> String,
{
    let mut by_source = std::collections::HashMap::new();
    for item in items.drain(..) {
        by_source.insert(source_path(&item), item);
    }

    let mut by_project_session: std::collections::HashMap<String, T> = std::collections::HashMap::new();
    for item in by_source.into_values() {
        let key = project_key(&item);
        let replace = match by_project_session.get(&key) {
            Some(existing) => modified_at(&item) >= modified_at(existing),
            None => true,
        };
        if replace {
            by_project_session.insert(key, item);
        }
    }

    let mut out: Vec<T> = by_project_session.into_values().collect();
    out.sort_by(|a, b| modified_at(b).cmp(&modified_at(a)));
    out
}
