use deno_core::error::AnyError;
use std::rc::Rc;

pub async fn run(file_path: &str) -> Result<(), AnyError> {
	let main_module = deno_core::resolve_path(file_path)?;
	let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
		module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
		..Default::default()
	});

	let mod_id = js_runtime.load_main_module(&main_module, None).await?;
	let result = js_runtime.mod_evaluate(mod_id);
	js_runtime.run_event_loop(false).await?;
	result.await?
}

pub fn init() {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	if let Err(error) = runtime.block_on(run("./example.js")) {
		eprintln!("error: {}", error);
	}
}
#[cfg(test)]
mod tests {
	use std::{fs, path::Path};

	

	use crate::utils::{BACKEND_DIST, PAGE_DIST};

	#[test]
	fn test() {
		let context = Context::new().unwrap();
		
		
		let code_path = Path::new(&BACKEND_DIST.clone()).join("format.js");
		let code = fs::read_to_string(&code_path).unwrap();
		
		let input = r#"# Test -> Joking

    Not really
    
    share: public r"#;

		let input = code.replace("INPUT_VALUE", input);
		let output = context.eval(&input).unwrap();

		assert_eq!(
			output,
			JsValue::Null
		)
	}
}
