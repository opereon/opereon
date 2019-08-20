use super::*;

/// Function to load file form git repository.
/// Path must be relative to repository dir.
#[derive(Debug, Clone)]
pub struct LoadFileFunc {
    model_dir: PathBuf,
    model_oid: Sha1Hash,
}

impl LoadFileFunc {
    pub fn new(model_dir: PathBuf, model_oid: Sha1Hash) -> Self {
        Self {
            model_dir,
            model_oid,
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

        let git = GitManager::new(&self.model_dir)
            .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
        let tree = git
            .get_tree(&self.model_oid.into())
            .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
        let odb = git
            .odb()
            .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;

        if args.count() == 1 {
            for path in paths.into_iter() {
                let path = PathBuf::from(path.as_string());
                let entry = tree
                    .get_path_ext(&path)
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
                let obj = odb
                    .read(entry.id())
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })
                    .into_diag_res()
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;

                let format = path.extension().map_or(FileFormat::Text, |ext| {
                    FileFormat::from(ext.to_str().unwrap())
                });

                let node = NodeRef::from_bytes(obj.data(), format)
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
                out.add(node)
            }
        } else {
            let formats = args
                .resolve_column(false, 1, env)
                .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;

            for (p, f) in paths.into_iter().zip(formats.into_iter()) {
                let path = PathBuf::from(p.as_string());
                let entry = tree
                    .get_path_ext(&path)
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;
                let obj = odb
                    .read(entry.id())
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })
                    .into_diag_res()
                    .map_err(|err| FuncCallErrorDetail::custom_func(&func_id, err))?;

                let format: FileFormat = f.data().as_string().as_ref().into();

                let node = NodeRef::from_bytes(obj.data(), format)
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
