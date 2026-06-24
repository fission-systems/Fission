use crate::nir::pass::{AnalysisStore, NirFunc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassResult {
    NoChange,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatMode {
    Once,
    UntilStable,
}

pub(crate) trait NirPass {
    fn name(&self) -> &str;
    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String>;
}

pub(crate) struct PassManager {
    passes: Vec<Box<dyn NirPass>>,
    repeat_mode: RepeatMode,
    max_rounds: usize,
}

impl PassManager {
    pub(crate) fn new(repeat_mode: RepeatMode, max_rounds: usize) -> Self {
        Self {
            passes: Vec::new(),
            repeat_mode,
            max_rounds,
        }
    }

    pub(crate) fn add_pass(&mut self, pass: Box<dyn NirPass>) {
        self.passes.push(pass);
    }

    pub(crate) fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        let mut overall_changed = PassResult::NoChange;
        let mut round = 0;

        loop {
            let mut round_changed = false;
            for pass in &mut self.passes {
                match pass.run(ir, store)? {
                    PassResult::Changed => {
                        round_changed = true;
                        overall_changed = PassResult::Changed;
                    }
                    PassResult::NoChange => {}
                }
            }

            round += 1;

            if self.repeat_mode == RepeatMode::Once || !round_changed || round >= self.max_rounds {
                break;
            }
        }

        Ok(overall_changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::builder::PreviewBuilder;
    use crate::nir::types::MlilPreviewOptions;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn make_dummy_builder() -> PreviewBuilder<'static> {
        let pcode = Box::leak(Box::new(PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x2000,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
            ],
        }));
        let options = Box::leak(Box::new(MlilPreviewOptions {
            is_64bit: true,
            pointer_size: 8,
            ..Default::default()
        }));
        PreviewBuilder::new(pcode, options, None)
    }

    struct MockPass {
        runs: Rc<RefCell<usize>>,
        limit: usize,
    }

    impl NirPass for MockPass {
        fn name(&self) -> &str {
            "MockPass"
        }

        fn run(
            &mut self,
            ir: &mut NirFunc<'_, '_>,
            _store: &mut AnalysisStore,
        ) -> Result<PassResult, String> {
            let mut runs = self.runs.borrow_mut();
            *runs += 1;
            if *runs < self.limit {
                // Mutate the successors to force version bump
                ir.successors_mut();
                Ok(PassResult::Changed)
            } else {
                Ok(PassResult::NoChange)
            }
        }
    }

    #[test]
    fn test_cache_invalidation_on_mutation() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        // 1. Initial access caches analyses
        assert_eq!(ir.cfg_version(), 0);
        let _facts1 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(0));

        // 2. Querying again with no mutation hits the cache (version remains 0)
        let _facts2 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(0));

        // 3. Mutating ir increments version
        ir.successors_mut();
        assert_eq!(ir.cfg_version(), 1);

        // 4. Querying again triggers invalidation and re-computation
        let _facts3 = store.cfg_facts(&ir);
        assert_eq!(store.cfg_version_for_test(), Some(1));
    }

    #[test]
    fn test_pass_manager_until_stable() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        let runs = Rc::new(RefCell::new(0));
        let mut pm = PassManager::new(RepeatMode::UntilStable, 10);
        pm.add_pass(Box::new(MockPass {
            runs: runs.clone(),
            limit: 3,
        }));

        let res = pm.run(&mut ir, &mut store).unwrap();
        assert_eq!(res, PassResult::Changed);
        assert_eq!(*runs.borrow(), 3);
    }

    #[test]
    fn test_pass_manager_once() {
        let mut builder = make_dummy_builder();
        let mut ir = NirFunc::new(&mut builder);
        let mut store = AnalysisStore::new();

        let runs = Rc::new(RefCell::new(0));
        let mut pm = PassManager::new(RepeatMode::Once, 10);
        pm.add_pass(Box::new(MockPass {
            runs: runs.clone(),
            limit: 3,
        }));

        let res = pm.run(&mut ir, &mut store).unwrap();
        assert_eq!(res, PassResult::Changed);
        assert_eq!(*runs.borrow(), 1);
    }
}
