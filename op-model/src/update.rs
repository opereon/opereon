use super::*;


#[derive(Debug)]
pub struct ModelUpdate<'a> {
    cache: NodePathCache,
    diff: Diff,
    matcher1_r: NodePathMatcher,
    matcher2_a: NodePathMatcher,
    matcher2_u: NodePathMatcher,
    model1: &'a Model,
    model2: &'a Model,
}

impl<'a> ModelUpdate<'a> {
    pub fn new(model1: &'a Model, model2: &'a Model) -> ModelUpdate<'a> {
        let mut cache = NodePathCache::new();
        let diff = Diff::full_cache(model1.root(), model2.root(), &mut cache);

        ModelUpdate {
            cache,
            diff,
            matcher1_r: NodePathMatcher::new(),
            matcher2_a: NodePathMatcher::new(),
            matcher2_u: NodePathMatcher::new(),
            model1,
            model2,
        }
    }

    pub fn diff(&self) -> &Diff {
        &self.diff
    }

    pub fn check_updater(&mut self, u: &ProcDef) -> Vec<&Change> {
        debug_assert!(u.kind() == ProcKind::Update);

        self.matcher1_r.clear();
        self.matcher2_a.clear();
        self.matcher2_u.clear();

        let root1 = self.model1.root();
        let scope1 = self.model1.scope();
        let root2 = self.model2.root();
        let scope2 = self.model2.scope();

        for w in u.watches().iter() {
            if w.mask().has_removed() {
                self.matcher1_r.resolve_ext_cache(w.path(), root1, root1, &scope1, &mut self.cache);
            }
            if w.mask().has_added() {
                self.matcher2_a.resolve_ext_cache(w.path(), root2, root2, &scope2, &mut self.cache);
            }
            if w.mask().has_updated() {
                self.matcher2_u.resolve_ext_cache(w.path(), root2, root2, &scope2, &mut self.cache);
            }
        }

        let mut changes = Vec::new();
        for c in self.diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => if self.matcher1_r.matches(c.path()) {
                    changes.push(c)
                }
                ChangeKind::Added => if self.matcher2_a.matches(c.path()) {
                    changes.push(c)
                }
                ChangeKind::Updated => if self.matcher2_u.matches(c.path()) {
                    changes.push(c)
                }
            }
        }

        changes
    }
}
