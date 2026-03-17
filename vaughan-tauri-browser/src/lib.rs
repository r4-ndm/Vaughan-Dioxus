//! Vaughan dApp browser library.
//!
//! Task 30.3: IPC client bootstrap (connect + handshake).

mod ipc;

use std::time::Duration;

use ipc::IpcClient;
use vaughan_ipc_types::IpcRequest;

#[derive(Debug, Clone)]
struct CliConfig {
    ipc_endpoint: String,
    token: String,
}

fn parse_args() -> Option<CliConfig> {
    let mut ipc = None::<String>;
    let mut token = None::<String>;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ipc" | "--ipc-endpoint" => ipc = it.next(),
            "--token" => token = it.next(),
            _ => {}
        }
    }

    Some(CliConfig {
        ipc_endpoint: ipc?,
        token: token?,
    })
}

pub fn run() {
    let cli = parse_args();

    tauri::Builder::default()
        .setup(move |_app| {
            if let Some(cli) = cli.clone() {
                tokio::spawn(async move {
                    let timeout = Duration::from_secs(3);
                    match IpcClient::connect(&cli.ipc_endpoint, &cli.token, timeout).await {
                        Ok(mut c) => {
                            eprintln!("IPC connected: {}", cli.ipc_endpoint);
                            let _ = c.request(1, IpcRequest::GetNetworkInfo, timeout).await;
                        }
                        Err(e) => {
                            eprintln!("IPC connect failed: {e}");
                        }
                    }
                });
            } else {
                eprintln!("IPC config missing. Start with --ipc <endpoint> --token <token>");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
