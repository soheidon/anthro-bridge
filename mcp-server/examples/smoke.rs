//! Real-API smoke test: sends a single `plan` request to DeepSeek V4 Pro using
//! the real prompt builders, then prints the returned plan to stdout.
//!
//! This is NOT run by `cargo test`. It requires `DEEPSEEK_API_KEY` and makes
//! one paid provider request. Run it with:
//!
//! ```bash
//! cargo run --example smoke
//! ```

use anthro_bridge_mcp_server::mcp::{build_system_prompt, build_user_prompt};
use anthro_bridge_mcp_server::provider::deepseek::DeepSeekProvider;
use anthro_bridge_mcp_server::provider::PlannerProvider;

#[tokio::main]
async fn main() {
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY is not set");
    let provider = DeepSeekProvider::new(api_key);

    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(
        "Add a --version flag that prints the program version and exits.",
        "Rust binary crate. Argument parsing uses clap. The version string is defined in Cargo.toml as 0.1.0.",
        None,
    );

    match provider.plan(&system_prompt, &user_prompt).await {
        Ok(plan) => {
            println!("{}", plan.text);
        }
        Err(err) => {
            eprintln!("smoke test failed: {err}");
            std::process::exit(1);
        }
    }
}
