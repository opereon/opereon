use super::*;

/// Function to load file form git repository.
/// Path must be relative to repository dir.
#[derive(Debug, Clone)]
pub struct LoadFileFunc {
    model_dir: PathBuf,
    current_dir: PathBuf,
}

impl LoadFileFunc {
    pub fn new(model_dir: PathBuf, current_dir: PathBuf) -> Self {
        Self {
            model_dir,
            current_dir,
        }
    }
}

impl FuncCallable for LoadFileFunc {
    fn call(&self, name: &str, args: Args, env: Env, out: &mut NodeBuf) -> FuncCallResult {
        let func_id = FuncId::Custom(name.to_string());
        args.check_count_func(&func_id, 1, 2)?;

        let paths = args
            .resolve_column(false, 0, env)
            .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;


        if args.count() == 1 {
            for path in paths.into_iter() {
                let path = self.resolve_path(&path);

                let node = NodeRef::from_file(&path, None)
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
                out.add(node)
            }
        } else {
            let formats = args
                .resolve_column(false, 1, env)
                .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;

            for (path, format) in paths.into_iter().zip(formats.into_iter()) {
                let path = self.resolve_path(&path);

                let format: FileFormat = format.data().as_string().as_ref().into();

                let node = NodeRef::from_file(&path, Some(format))
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
                out.add(node)
            }
        }

        Ok(())
    }

    fn clone(&self) -> Box<dyn FuncCallable> {
        Box::new(std::clone::Clone::clone(self))
    }
}

impl LoadFileFunc {
    fn resolve_path<'a>(&self, path: &NodeRef) -> PathBuf {
        let path = PathBuf::from(path.as_string());
        resolve_model_path(path, &self.current_dir, &self.model_dir)
    }
}
