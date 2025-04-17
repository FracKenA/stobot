mod news;
mod handler;
mod arc_api;

use std::collections::BTreeSet;
use std::env;
use clap::Parser;
use serenity::prelude::*;
use crate::handler::Handler;
use chrono::Local; // Add this import for timestamps
use tokio::signal;

#[derive(Parser)]
struct Args {
    /// Path to saved channels
    #[clap(short, long, default_value = "channels.txt")]
    channels_path: String,

    /// Time in seconds inbetween checking for news
    #[clap(long, default_value_t = 10)]
    poll_period: u64,

    /// Number of news to poll in each period
    #[clap(long, default_value_t = 20)]
    poll_count: u64,

    /// Maximum time difference in seconds between now and timestamp of news item to be even considered for posting
    #[clap(short, long, default_value_t = 120)]
    fresh_seconds: u64,

    /// Amount of Discord messages to check for already posted news items during each poll. Discord has a limitation of 100.
    #[clap(short, long, default_value_t = 50)]
    msg_count: u8,

    /// Space separated list of platforms to filter news from. E.g.: to have news from all 3: `pc ps xbox`
    #[clap(default_values_t = vec!["pc".to_string(), "xbox".to_string(), "ps".to_string()], num_args = 0..)]
    platforms: Vec<String>
}

#[tokio::main]
async fn main() {
    let _name = env!("CARGO_PKG_NAME", "");
    let _version = env!("CARGO_PKG_VERSION", "");
    println!("CEF:0|stobot|{}|{}|INFO|Application started|time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), Local::now().to_rfc3339());
    
    // Using only the intents necessary for slash commands
    let intents = GatewayIntents::GUILD_MESSAGES 
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILD_INTEGRATIONS;
    
    let mut args = Args::parse();
    if let Ok(env_poll) = std::env::var("POLL_PERIOD") {
        if let Ok(val) = env_poll.parse::<u64>() {
            args.poll_period = val;
        }
    }
    println!("CEF:0|stobot|{}|{}|INFO|Saved channels path|msg={} time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.channels_path, Local::now().to_rfc3339());
    println!("CEF:0|stobot|{}|{}|INFO|Polling period|msg={} time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.poll_period, Local::now().to_rfc3339());
    println!("CEF:0|stobot|{}|{}|INFO|Poll count|msg={} time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.poll_count, Local::now().to_rfc3339());
    println!("CEF:0|stobot|{}|{}|INFO|Fresh seconds|msg={} time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.fresh_seconds, Local::now().to_rfc3339());
    println!("CEF:0|stobot|{}|{}|INFO|Messages to check|msg={} time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.msg_count, Local::now().to_rfc3339());
    
    // Create default platforms set with all platforms enabled
    let default_platforms = BTreeSet::from_iter(vec!["pc".to_string(), "xbox".to_string(), "ps".to_string()]);
    
    let handler = Handler::new(
        args.poll_period,
        args.poll_count,
        args.channels_path,
        args.fresh_seconds,
        args.msg_count,
        default_platforms,
    );
    
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN environment variable is unset!");
    let mut client =
        Client::builder(&token, intents).event_handler(handler).await.expect("Err creating client");

    // Spawn the client in a background task
    let client_handle = tokio::spawn(async move {
        if let Err(why) = client.start().await {
            eprintln!("CEF:0|stobot|{}|{}|ERROR|Client error|msg=Failed to start client. Error: {} | Context: Starting Discord client with token. time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), why, Local::now().to_rfc3339());
        }
    });

    // Wait for shutdown signal
    signal::ctrl_c().await.expect("Failed to listen for shutdown signal");
    println!("CEF:0|stobot|{}|{}|INFO|Shutdown signal received|time={}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), Local::now().to_rfc3339());

    // Optionally, wait for the client to finish
    let _ = client_handle.await;
}
