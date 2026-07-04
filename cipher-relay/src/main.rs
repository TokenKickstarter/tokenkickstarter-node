use cipher_relay::run_relay_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    // Parse arguments or default port 4002
    let port: u16 = std::env::args().nth(1).unwrap_or_else(|| "4002".to_string()).parse()?;
    
    run_relay_server(port).await
}
