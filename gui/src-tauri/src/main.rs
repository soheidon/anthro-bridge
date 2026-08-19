// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if anthro_bridge_lib::is_mcp_server_mode(&args) {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[anthro-bridge] failed to initialize tokio runtime for MCP: {e}");
                std::process::exit(1);
            }
        };

        match rt.block_on(anthro_bridge_mcp_server::run_stdio_mcp_server()) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[anthro-bridge] MCP server exited with error: {e}");
                std::process::exit(1);
            }
        }
    }

    anthro_bridge_lib::run()
}
