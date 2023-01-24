use lib::utils::log_env;

#[tokio::main]
async fn main() {
	println!("Running auth");
	env_logger::init();
	log_env();
}
