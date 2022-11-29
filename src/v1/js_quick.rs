// use std::fs;

// use quick_js::{Context, JsValue};

// use crate::utils::PAGE_DIST;

// // pub fn format(value: String) -> String {


// // 	// context.add_callback("myCallback", |a: i32, b: i32| a + b).unwrap();

// // 	if let Ok(path) = fs::read_to_string(PAGE_DIST.clone()) {


// //   }


// // 	let value = context.eval("1 + 2").unwrap();
// // 	assert_eq!(value, JsValue::Int(3));


// // 	assert_eq!(&value, "350");

// // 	// Callbacks.


// // 	context
// // 		.eval(
// // 			r#"
// //     // x will equal 30
// //     var x = myCallback(10, 20);
// // "#,
// // 		)
// // 		.unwrap();
// // }


// #[cfg(test)]
// mod tests {
// 	use std::{fs, path::Path};

// 	use quick_js::{Context, JsValue};

// 	use crate::utils::{BACKEND_DIST, PAGE_DIST};

// 	#[test]
// 	fn test() {
// 		let context = Context::new().unwrap();
		
		
// 		let code_path = Path::new(&BACKEND_DIST.clone()).join("format.js");

// 		let code = fs::read_to_string(&code_path).unwrap();
// 		let input = r#"# Test -> Joking

//     Not really
    
//     share: public r"#;

// 		let input = code.replace("INPUT_VALUE", input);
// 		let output = context.eval(&input).unwrap();

// 		assert_eq!(
// 			output,
// 			JsValue::Null
// 		)
// 	}
// }
