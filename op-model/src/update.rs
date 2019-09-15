use super::*;

#[derive(Debug)]
pub struct ModelUpdate<'a> {
    cache: NodePathCache,
    model_diff: NodeDiff,
    file_diff: FileDiff,
    model1: &'a Model,
    model2: &'a Model,
    matcher1_r: NodePathMatcher,
    matcher1_m: NodePathMatcher,
    matcher2_a: NodePathMatcher,
    matcher2_u: NodePathMatcher,
    matcher2_m: NodePathMatcher,
}

impl<'a> ModelUpdate<'a> {
    pub fn new(
        model1: &'a Model,
        model2: &'a Model,
        opts: &NodeDiffOptions,
        file_diff: FileDiff,
    ) -> ModelResult<ModelUpdate<'a>> {
        let mut cache = NodePathCache::new();
        let model_diff = NodeDiff::diff_cache(model1.root(), model2.root(), opts, &mut cache);
        let update = ModelUpdate {
            cache,
            model_diff,
            file_diff,
            model1,
            model2,
            matcher1_r: NodePathMatcher::new(),
            matcher1_m: NodePathMatcher::new(),
            matcher2_a: NodePathMatcher::new(),
            matcher2_u: NodePathMatcher::new(),
            matcher2_m: NodePathMatcher::new(),
        };
        Ok(update)
    }

    pub fn model_diff(&self) -> &NodeDiff {
        &self.model_diff
    }

    pub fn file_diff(&self) -> &FileDiff {
        &self.file_diff
    }

    pub fn check_updater(
        &mut self,
        u: &ProcDef,
    ) -> ModelResult<(Vec<&NodeChange>, Vec<&FileChange>)> {
        debug_assert!(u.kind() == ProcKind::Update);

        let root1 = self.model1.root();
        let scope1 = self.model1.scope()?;
        let root2 = self.model2.root();
        let scope2 = self.model2.scope()?;

        self.matcher1_r.clear();
        self.matcher1_m.clear();
        self.matcher2_a.clear();
        self.matcher2_u.clear();
        self.matcher2_m.clear();

        for mw in u.model_watches().iter() {
            if mw.mask().has_removed() {
                self.matcher1_r
                    .resolve_ext_cache(mw.path(), root1, root1, &scope1, &mut self.cache)
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;
            }
            if mw.mask().has_added() {
                self.matcher2_a
                    .resolve_ext_cache(mw.path(), root2, root2, &scope2, &mut self.cache)
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;
            }
            if mw.mask().has_updated() {
                self.matcher2_u
                    .resolve_ext_cache(mw.path(), root2, root2, &scope2, &mut self.cache)
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;
            }
            if mw.mask().has_moved() {
                self.matcher1_m
                    .resolve_ext_cache(mw.path(), root1, root1, &scope1, &mut self.cache)
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;
                self.matcher2_m
                    .resolve_ext_cache(mw.path(), root2, root2, &scope2, &mut self.cache)
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;
            }
        }

        let mut model_changes = Vec::new();
        for c in self.model_diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => {
                    if self.matcher1_r.matches(c.old_path().unwrap()) {
                        model_changes.push(c)
                    }
                }
                ChangeKind::Added => {
                    if self.matcher2_a.matches(c.new_path().unwrap()) {
                        model_changes.push(c)
                    }
                }
                ChangeKind::Updated => {
                    if self.matcher2_u.matches(c.new_path().unwrap()) {
                        model_changes.push(c)
                    }
                }
                ChangeKind::Moved => {
                    if self.matcher1_m.matches(c.old_path().unwrap())
                        || self.matcher2_m.matches(c.new_path().unwrap()) {
                        model_changes.push(c)
                    }
                },
            }
        }

        let fw: &[FileWatch] = u.file_watches();

        let removed_watches: Vec<&FileWatch> =
            fw.iter().filter(|w| w.mask().has_removed()).collect();
        let added_watches: Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_added()).collect();
        let updated_watches: Vec<&FileWatch> =
            fw.iter().filter(|w| w.mask().has_updated()).collect();
        let renamed_watches: Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_moved()).collect();

        let mut file_changes = Vec::new();
        for c in self.file_diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => {
                    removed_watches
                        .iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.old_path().unwrap()))
                        .for_each(|_w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Added => {
                    added_watches
                        .iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.new_path().unwrap()))
                        .for_each(|_w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Updated => {
                    updated_watches
                        .iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.new_path().unwrap()))
                        .for_each(|_w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Moved => {
                    renamed_watches
                        .iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.old_path().unwrap()))
                        .for_each(|_w| {
                            file_changes.push(c);
                        });
                }
            }
        }
        Ok((model_changes, file_changes))
    }
}
