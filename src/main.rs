use clap::Parser;
use std::{env, process};

use crate::modules::agent::Agent;

mod modules;

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });
    
    let mut agent = Agent::new(&base_url, &api_key)?;

    let response = agent.handle_request(&args.prompt).await?;
    println!("{}", response);

    Ok(())
}
