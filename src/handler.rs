use std::collections::{BTreeSet, BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use serenity::async_trait;
use serenity::builder::{CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage, GetMessages, CreateEmbed, CreateEmbedFooter, CreateInteractionResponseFollowup, CreateAttachment};
use serenity::all::{
    Interaction, CommandOptionType,
    CommandInteraction, Command,
};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
// use serenity::futures::Future;
use regex::Regex;
use tokio::time::{self};
use chrono::Local;
use scraper::{Html, Selector};

use crate::news::News;
use crate::arc_api::build_news_url;

// Helper for logging errors with consistent format
fn log_error(context: &str, error: impl std::fmt::Display) {
    eprintln!("CEF:0|stobot|{}|{}|ERROR|Error|msg={} | Context: {}. time={}", 
        env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), error, context, Local::now().to_rfc3339());
}

// Helper for logging info with consistent format
fn log_info(message: &str, details: Option<&str>) {
    if let Some(details) = details {
        println!("CEF:0|stobot|{}|{}|INFO|{}|msg={} time={}", 
            env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), message, details, Local::now().to_rfc3339());
    } else {
        println!("CEF:0|stobot|{}|{}|INFO|{}|time={}", 
            env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), message, Local::now().to_rfc3339());
    }
}

pub struct Handler {
    poll_period: u64,
    poll_count: u64,
    channel_ids: Mutex<HashSet<u64>>,
    channel_txt_path: String,
    fresh_seconds: u64,
    msg_count: u8,
    platforms: Mutex<BTreeMap<u64, BTreeSet<String>>>, // Map of channel ID to platforms
}

impl Handler {
    pub fn new(poll_period: u64, poll_count: u64, channel_txt_path: String, fresh_seconds: u64, msg_count: u8, _default_platforms: BTreeSet<String>) -> Handler {
        // Default platforms to include all three: pc, xbox, ps
        let default_platforms = BTreeSet::from_iter(vec!["pc".to_string(), "xbox".to_string(), "ps".to_string()]);
        let handler = Handler {
            poll_period,
            poll_count,
            channel_ids: Mutex::new(HashSet::new()),
            channel_txt_path,
            fresh_seconds,
            msg_count,
            platforms: Mutex::new(BTreeMap::new()),
        };
        log_info("Reading channels file", Some(&handler.channel_txt_path));
        
        if let Ok(file) = File::open(&handler.channel_txt_path) {
            let reader = BufReader::new(file);
            let mut channel_ids = handler.channel_ids.lock().unwrap();
            let mut platforms_map = handler.platforms.lock().unwrap();
            
            for line in reader.lines() {
                if let Ok(line) = line {
                    if line.starts_with("channel:") {
                        // Parse channel entry: channel:123456789|pc,ps,xbox
                        let parts: Vec<&str> = line.trim_start_matches("channel:").split('|').collect();
                        if parts.len() == 2 {
                            if let Ok(channel_id) = parts[0].parse::<u64>() {
                                let platform_list: Vec<String> = parts[1].split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                
                                let platform_set = if !platform_list.is_empty() {
                                    BTreeSet::from_iter(platform_list)
                                } else {
                                    default_platforms.clone()
                                };
                                
                                // Add to channel_ids and platforms
                                channel_ids.insert(channel_id);
                                platforms_map.insert(channel_id, platform_set.clone());
                                
                                log_info("Loaded channel", Some(&format!("ID:{} Platforms:{:?}", channel_id, platform_set)));
                            }
                        }
                    }
                }
            }
        } else {
            log_error("Reading channels file", format!("Could not open file: {}", handler.channel_txt_path));
        }
        
        log_info("Channels", None);
        let channels = handler.get_channels();
        for channel in channels.iter() {
            print!(" {channel}");
        }
        println!(" end=time:{}", Local::now().to_rfc3339());
        
        handler
    }
    
    fn add_channel_internal(&self, id: u64, platforms: BTreeSet<String>, platforms_map: &mut MutexGuard<BTreeMap<u64, BTreeSet<String>>>) {
        let mut channel_ids = self.channel_ids.lock().unwrap();
        channel_ids.insert(id);
        platforms_map.insert(id, platforms);
        self.write_channels_to_file(&channel_ids, platforms_map);
    }

    pub fn get_channels(&self) -> HashSet<u64> {
        self.channel_ids.lock().unwrap().clone()
    }

    fn write_channels_to_file(&self, channel_ids: &MutexGuard<HashSet<u64>>, platforms_map: &MutexGuard<BTreeMap<u64, BTreeSet<String>>>) {
        let mut file = File::create(&self.channel_txt_path).expect(format!(
            "Couldn't open {}", self.channel_txt_path).as_str());
        
        // Write each channel and its platforms
        for id in channel_ids.iter() {
            let default_platforms = BTreeSet::new();
            let platforms = platforms_map.get(id).unwrap_or(&default_platforms);
            let platforms_str = platforms.iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(",");
            
            writeln!(file, "channel:{}|{}", id, platforms_str).expect(format!(
                "Couldn't write to {}", self.channel_txt_path).as_str());
        }
    }

    pub fn add_channel(&self, id: u64) {
        let mut platforms_map = self.platforms.lock().unwrap();
        // Use all three platforms by default
        let default_platforms = BTreeSet::from_iter(vec!["pc".to_string(), "xbox".to_string(), "ps".to_string()]);
        self.add_channel_internal(id, default_platforms, &mut platforms_map);
    }

    fn remove_channel(&self, id: u64) {
        let mut channel_ids = self.channel_ids.lock().unwrap();
        channel_ids.remove(&id);
        
        let mut platforms_map = self.platforms.lock().unwrap();
        platforms_map.remove(&id);
        
        self.write_channels_to_file(&channel_ids, &platforms_map);
    }

    fn get_channel_platforms(&self, channel_id: u64) -> BTreeSet<String> {
        let platforms_map = self.platforms.lock().unwrap();
        platforms_map.get(&channel_id)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from_iter(vec!["pc".to_string(), "xbox".to_string(), "ps".to_string()]))
    }

    fn update_channel_platforms(&self, channel_id: u64, new_platforms: BTreeSet<String>) {
        let mut platforms_map = self.platforms.lock().unwrap();
        platforms_map.insert(channel_id, new_platforms);
        
        let channel_ids = self.channel_ids.lock().unwrap();
        self.write_channels_to_file(&channel_ids, &platforms_map);
    }

    fn get_ids_from_messages(messages: &Vec<Message>) -> Vec<u64> {
        let mut result: Vec<u64> = vec![];
        // Regex to find IDs in embed URLs like https://playstartrekonline.com/en/news/article/1234567
        let re_embed_url = Regex::new(r"playstartrekonline\.com/en/news/article/(\d+)").unwrap();
        // Original regex for message content (kept as fallback or for other potential ID formats)
        let re_content = Regex::new(r"ID:(\d+)").unwrap(); // Adjusted to look for "ID:12345" pattern if needed, or keep original if that was intended. Let's assume URL is primary now.

        for m in messages {
            // Check embeds first
            for embed in &m.embeds {
                if let Some(url) = &embed.url {
                    if let Some(capture) = re_embed_url.captures(url) {
                        if let Ok(id) = capture[1].parse::<u64>() {
                            result.push(id);
                            // Assuming one news item per message, break after finding ID in embed
                            break; 
                        }
                    }
                }
            }

            // If not found in embed, check content (optional fallback)
            if let Some(capture) = re_content.captures(m.content.as_str()) {
                if let Ok(id) = capture[1].parse::<u64>() {
                    result.push(id);
                }
            }
        }
        // Remove duplicates if necessary, although the logic should prevent adding the same ID twice per message
        result.sort_unstable();
        result.dedup();
        result
    }

    async fn fetch_and_filter_news(&self, tag: Option<&str>, limit: u32, platforms: &BTreeSet<String>) -> Option<News> {
        let url = build_news_url(
            tag,
            Some(limit),
            Some(0),
            None,
            &["images.img_microsite_thumbnail", "platforms", "updated"]
        );
        
        match reqwest::get(&url).await {
            Ok(resp) => match resp.text().await {
                Ok(text) => {
                    match serde_json::from_str::<crate::news::News>(&text) {
                        Ok(mut news) => {
                            if news.filter_news_by_platform(platforms) {
                                Some(news)
                            } else {
                                None
                            }
                        },
                        Err(why) => {
                            log_error("Parsing news from API", why);
                            None
                        }
                    }
                },
                Err(why) => {
                    log_error("Reading text from API response", why);
                    None
                }
            },
            Err(why) => {
                log_error("Fetching news from API", why);
                None
            }
        }
    }

    async fn register_commands(&self, ctx: &Context) {
        // Remove all existing global commands before registering new ones
        let _ = Command::set_global_commands(&ctx.http, vec![]);
        
        // Define admin-only commands with permission restrictions
        let admin_commands = vec![
            CreateCommand::new("stobot_register")
                .description("Register this channel for STO news")
                .default_member_permissions(serenity::model::permissions::Permissions::ADMINISTRATOR),
            CreateCommand::new("stobot_unregister")
                .description("Unregister this channel from STO news")
                .default_member_permissions(serenity::model::permissions::Permissions::ADMINISTRATOR),
            CreateCommand::new("stobot_setplatforms")
                .description("Set monitored platforms for this channel")
                .default_member_permissions(serenity::model::permissions::Permissions::ADMINISTRATOR)
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "platforms", "Comma-separated platforms (pc,ps,xbox)")
                        .required(true)
                ),
            CreateCommand::new("stobot_status")
                .description("Show current bot configuration")
                .default_member_permissions(serenity::model::permissions::Permissions::ADMINISTRATOR),
        ];
        
        // Define regular commands available to all users
        let user_commands = vec![
            CreateCommand::new("stobot_help")
                .description("Show available commands"),
            CreateCommand::new("stobot_patchnotes")
                .description("Show recent patch notes for STO")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "platforms", "Platforms to filter by (pc,ps,xbox). Default: use channel settings")
                        .required(false)
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "weeks", "Number of weeks to look back (default: 1)")
                        .required(false)
                        .min_int_value(1)
                        .max_int_value(52)
                ),
            CreateCommand::new("stobot_news")
                .description("Show recent STO news items")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "platforms", "Platforms to filter by (pc,ps,xbox). Default: use channel settings")
                        .required(false)
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "weeks", "Number of weeks to look back (default: 1)")
                        .required(false)
                        .min_int_value(1)
                        .max_int_value(52)
                ),
            CreateCommand::new("stobot_wiki")
                .description("Search STOWiki.net for information (private reply)")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "query", "Search term or article name")
                        .required(true)
                ),
            CreateCommand::new("stobot_wiki_shared")
                .description("Search STOWiki.net for information (shared in channel)")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "query", "Search term or article name")
                        .required(true)
                ),
        ];
        
        // Combine all commands
        let all_commands = [admin_commands, user_commands].concat();

        log_info("Registering slash commands", None);
            
        // Clear existing commands and set new ones
        if let Err(e) = Command::set_global_commands(&ctx.http, all_commands).await {
            log_error("Registering global commands", e);
        } else {
            log_info("Slash commands registered successfully", None);
        }
    }

    async fn get_and_show_news(&self, ctx: &Context, command: &CommandInteraction, tag: Option<&str>, title: &str, limit: u32, weeks: u32, exclude_tags: Option<Vec<&str>>, platforms: Option<BTreeSet<String>>) -> Result<(), serenity::Error> {
        let platforms = platforms.unwrap_or_else(|| self.get_channel_platforms(command.channel_id.get()));
        
        log_info("Fetching news", Some(&format!("Channel: {}, Tag: {:?}, Platforms: {:?}, Weeks: {}", command.channel_id.get(), tag, platforms, weeks)));
        
        // Use the helper function to fetch and filter news
        match self.fetch_and_filter_news(tag, limit, &platforms).await {
            Some(news) => {
                let mut embeds = Vec::new();
                let mut found_items = 0;
                
                // Create embeds for items within the specified time period
                for item in news.iter().filter(|item| item.is_within_weeks(weeks) && exclude_tags.as_ref().map_or(true, |tags| !tags.contains(&item.get_tag()))) {
                    found_items += 1;
                    let (summary, icon_files) = item.format_with_platforms(&platforms);
                    let mut embed = CreateEmbed::default()
                        .title(item.get_title())
                        .description(summary)
                        .url(&format!("https://playstartrekonline.com/en/news/article/{}", item.get_id()));
                    if let Some(img_url) = item.get_thumbnail_url() {
                        embed = embed.thumbnail(img_url);
                    }
                    if let Some(icon_path) = icon_files.get(0) {
                        embed = embed.image(format!("attachment://{}", icon_path.split('/').last().unwrap()));
                    }
                    embeds.push(embed);
                    if found_items >= limit as usize {
                        break;
                    }
                }

                // When sending the response, add the icon files as attachments
                if !embeds.is_empty() {
                    let mut msg = CreateInteractionResponseMessage::new()
                        .content(format!("{} Found {} {} from the last {} {} (Platforms: {:?})", 
                            title, 
                            found_items,
                            if found_items == 1 { "item" } else { "items" },
                            weeks,
                            if weeks == 1 { "week" } else { "weeks" },
                            platforms))
                        .embeds(embeds)
                        .ephemeral(true);
                    let mut all_icon_files = Vec::new();
                    for item in news.iter() {
                        let (_, icon_files) = item.format_with_platforms(&platforms);
                        for icon in icon_files {
                            if !all_icon_files.contains(&icon) {
                                all_icon_files.push(icon);
                            }
                        }
                    }
                    for icon_path in all_icon_files {
                        if let Ok(attachment) = CreateAttachment::path(icon_path.clone()).await {
                            msg = msg.add_file(attachment);
                        }
                    }
                    command.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await
                } else {
                    command.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!("No {} found from the last {} {} for platforms: {:?}", 
                                match tag {
                                    Some("patch-notes") => "patch notes",
                                    Some("star-trek-online") => "news",
                                    _ => "announcements"
                                }, 
                                weeks,
                                if weeks == 1 { "week" } else { "weeks" },
                                platforms))
                            .ephemeral(true) // Makes this response only visible to the user who issued the command
                    )).await
                }
            },
            None => {
                command.create_response(&ctx.http, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(format!("No {} found for platforms: {:?}", 
                            if tag.is_some() { "news" } else { "announcements" }, 
                            platforms))
                        .ephemeral(true) // Makes this response only visible to the user who issued the command
                )).await
            }
        }
    }

    async fn handle_slash_command(&self, ctx: &Context, command: &CommandInteraction) -> Result<(), serenity::Error> {
        let content = match command.data.name.as_str() {
            "stobot_register" => {
                let id = command.channel_id.get();
                self.add_channel(id);
                log_info("Registered channel", Some(&format!("ID:{}", id)));
                format!("This channel (ID: {}) will now have STO news posted.", id)
            },
            "stobot_unregister" => {
                let id = command.channel_id.get();
                self.remove_channel(id);
                log_info("Removed channel", Some(&format!("ID:{}", id)));
                format!("This channel (ID: {}) will no longer have STO news posted.", id)
            },
            "stobot_status" => {
                let channel_id = command.channel_id.get();
                let platforms = self.get_channel_platforms(channel_id);
                let is_registered = self.channel_ids.lock().unwrap().contains(&channel_id);
                
                if is_registered {
                    format!(
                        "📊 **Bot Status**\n• Polling Period: {} seconds (set via environment variable)\n• This Channel's Platforms: {:?}\n• This Channel: Registered",
                        self.poll_period, platforms
                    )
                } else {
                    format!(
                        "📊 **Bot Status**\n• Polling Period: {} seconds (set via environment variable)\n• This Channel: Not Registered\n• Use `/stobot_register` to register this channel",
                        self.poll_period
                    )
                }
            },
            "stobot_help" => {
                "📖 **Available Commands**\n\
                **Admin Commands** (requires Administrator permission):\n\
                • `/stobot_register` - Register this channel for STO news\n\
                • `/stobot_unregister` - Unregister this channel\n\
                • `/stobot_status` - Show current configuration\n\
                • `/stobot_setplatforms <platforms>` - Set monitored platforms (comma-separated, e.g., pc,ps,xbox)\n\n\
                **General Commands**:\n\
                • `/stobot_news [platforms](Defaults to all Platforms) [weeks](Defaults to 1 Week)` - Show recent STO news (excluding patch notes)\n\
                • `/stobot_patchnotes [platforms](Defaults to all Platforms) [weeks](Defaults to 1 Week)` - Show recent STO patch notes\n\
                • `/stobot_wiki <query>` - Search STOWiki.net for information (private reply)\n\
                • `/stobot_wiki_shared <query>` - Search STOWiki.net for information (shared in channel)\n\
                • `/stobot_help` - Show this help message".to_string()
            },
            "stobot_setplatforms" => {
                let channel_id = command.channel_id.get();
                let options = &command.data.options;
                if let Some(option) = options.get(0) {
                    if let Some(platforms_str) = option.value.as_str() {
                        let platforms: Vec<String> = platforms_str.split(',').map(|s| s.trim().to_string()).collect();
                        let platform_set: BTreeSet<String> = platforms.into_iter().collect();
                        
                        if platform_set.is_empty() {
                            "Platform list cannot be empty.".to_string()
                        } else {
                            self.update_channel_platforms(channel_id, platform_set.clone());
                            format!("Monitored platforms for this channel updated to {:?}.", platform_set)
                        }
                    } else {
                        "Invalid platforms value provided".to_string()
                    }
                } else {
                    "Missing platforms parameter".to_string()
                }
            },
            "stobot_patchnotes" => {
                // Parse platforms parameter if provided
                let platforms = if let Some(platform_option) = command.data.options.iter().find(|opt| opt.name == "platforms") {
                    if let Some(platforms_str) = platform_option.value.as_str() {
                        let platform_set: BTreeSet<String> = platforms_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        
                        if !platform_set.is_empty() {
                            platform_set
                        } else {
                            self.get_channel_platforms(command.channel_id.get())
                        }
                    } else {
                        self.get_channel_platforms(command.channel_id.get())
                    }
                } else {
                    self.get_channel_platforms(command.channel_id.get())
                };
                
                // Get the weeks parameter or default to 1
                let weeks = command.data.options.iter()
                    .find(|opt| opt.name == "weeks")
                    .and_then(|opt| opt.value.as_i64())
                    .unwrap_or(1) as u32;
                
                self.get_and_show_news(ctx, command, Some("patch-notes"), &format!("**STO Patch Notes (last {} {})**:", weeks, if weeks == 1 { "week" } else { "weeks" }), 20, weeks, None, Some(platforms)).await?;
                return Ok(());
            },
            "stobot_news" => {
                // Parse platforms parameter if provided
                let platforms = if let Some(platform_option) = command.data.options.iter().find(|opt| opt.name == "platforms") {
                    if let Some(platforms_str) = platform_option.value.as_str() {
                        let platform_set: BTreeSet<String> = platforms_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        
                        if !platform_set.is_empty() {
                            platform_set
                        } else {
                            self.get_channel_platforms(command.channel_id.get())
                        }
                    } else {
                        self.get_channel_platforms(command.channel_id.get())
                    }
                } else {
                    self.get_channel_platforms(command.channel_id.get())
                };
                
                // Get the weeks parameter or default to 1
                let weeks = command.data.options.iter()
                    .find(|opt| opt.name == "weeks")
                    .and_then(|opt| opt.value.as_i64())
                    .unwrap_or(1) as u32;
                
                self.get_and_show_news(ctx, command, Some("star-trek-online"), &format!("**STO News (last {} {})**:", weeks, if weeks == 1 { "week" } else { "weeks" }), 20, weeks, Some(vec!["patch-notes"]), Some(platforms)).await?;
                return Ok(());
            },
            "stobot_wiki" => {
                let options = command.data.options.get(0);
                let query = options
                    .and_then(|opt| opt.value.as_str())
                    .unwrap_or("");
                self.handle_wiki_search(ctx, command, query).await?;
                return Ok(());
            },
            "stobot_wiki_shared" => {
                let options = command.data.options.get(0);
                let query = options
                    .and_then(|opt| opt.value.as_str())
                    .unwrap_or("");
                self.handle_wiki_search_shared(ctx, command, query).await?;
                return Ok(());
            },
            _ => "Unknown command".to_string(),
        };

        command
            .create_response(&ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true) // Makes all slash command responses only visible to the user who issued the command
            ))
            .await
    }

    async fn handle_wiki_search(&self, ctx: &Context, command: &CommandInteraction, query: &str) -> Result<(), serenity::Error> {
        command.defer(&ctx.http).await?;
        
        if query.is_empty() {
            command.create_followup(&ctx.http, CreateInteractionResponseFollowup::new()
                .content("Please provide a search term for the STOWiki.")
                .ephemeral(true)).await?;
            return Ok(());
        }
        
        // Build the STOWiki search URL
        let search_url = format!("https://stowiki.net/wiki/Special:Search?search={}&go=Go", 
            query.replace(" ", "%20"));
        
        // For direct article URL (if query matches article exactly)
        let direct_article_url = format!("https://stowiki.net/wiki/{}", 
            query.replace(" ", "_"));
        
        // Try to fetch the article and extract a preview
        let preview = match reqwest::get(&direct_article_url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(body) = resp.text().await {
                        let document = Html::parse_document(&body);
                        let selector = Selector::parse("#mw-content-text > div.mw-parser-output > p").unwrap();
                        if let Some(element) = document.select(&selector).next() {
                            let text = element.text().collect::<Vec<_>>().join("").trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            Err(_) => None,
        };
        
        // Create the response with relevant links and preview
        let description = match &preview {
            Some(preview) => format!(
                "[View search results]({}) | [Try direct article link]({})\n\n{}\n\nSTOWiki.net is the community Star Trek Online wiki.",
                search_url, direct_article_url, preview
            ),
            None => format!(
                "[View search results]({}) | [Try direct article link]({})\n\nSTOWiki.net is the community Star Trek Online wiki.",
                search_url, direct_article_url
            ),
        };
        
        let response = CreateInteractionResponseFollowup::new()
            .embed(CreateEmbed::new()
                .title(format!("STOWiki Search: {}", query))
                .description(description)
                .color(0x00ADEF) // STO blue color
                .footer(CreateEmbedFooter::new("Results from STOWiki.net")));
        
        command.create_followup(&ctx.http, response).await?;
        Ok(())
    }

    async fn handle_wiki_search_shared(&self, ctx: &Context, command: &CommandInteraction, query: &str) -> Result<(), serenity::Error> {
        command.defer(&ctx.http).await?;
        
        if query.is_empty() {
            command.create_followup(&ctx.http, CreateInteractionResponseFollowup::new()
                .content("Please provide a search term for the STOWiki.")
                .ephemeral(true)).await?;
            return Ok(());
        }
        
        // Build the STOWiki search URL
        let search_url = format!("https://stowiki.net/wiki/Special:Search?search={}&go=Go", 
            query.replace(" ", "%20"));
        
        // For direct article URL (if query matches article exactly)
        let direct_article_url = format!("https://stowiki.net/wiki/{}", 
            query.replace(" ", "_"));
        
        // Try to fetch the article and extract a preview
        let preview = match reqwest::get(&direct_article_url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(body) = resp.text().await {
                        let document = Html::parse_document(&body);
                        let selector = Selector::parse("#mw-content-text > div.mw-parser-output > p").unwrap();
                        if let Some(element) = document.select(&selector).next() {
                            let text = element.text().collect::<Vec<_>>().join("").trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            Err(_) => None,
        };
        
        // Create the response with relevant links and preview
        let description = match &preview {
            Some(preview) => format!(
                "[View search results]({}) | [Try direct article link]({})\n\n{}\n\nSTOWiki.net is the community Star Trek Online wiki.",
                search_url, direct_article_url, preview
            ),
            None => format!(
                "[View search results]({}) | [Try direct article link]({})\n\nSTOWiki.net is the community Star Trek Online wiki.",
                search_url, direct_article_url
            ),
        };
        
        let response = CreateInteractionResponseFollowup::new()
            .embed(CreateEmbed::new()
                .title(format!("STOWiki Search: {}", query))
                .description(description)
                .color(0x00ADEF) // STO blue color
                .footer(CreateEmbedFooter::new("Results from STOWiki.net")));
        
        command.create_followup(&ctx.http, response).await?;
        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, _msg: Message) {
        // Legacy !commands are no longer supported
        // All commands should use slash commands instead
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log_info("Bot connected", Some(&format!("Connected as {} ({})", ready.user.name, ready.user.id)));
        
        // Register slash commands
        self.register_commands(&ctx).await;

        loop {
            for channel_id in self.get_channels().iter() {
                let channel_platforms = self.get_channel_platforms(*channel_id);
                if let Some(news) = self.fetch_and_filter_news(None, self.poll_count as u32, &channel_platforms).await {
                    let channel = ChannelId::new(*channel_id);
                    let builder = GetMessages::new().limit(self.msg_count);
                    if let Ok(existing_messages) = channel.messages(&ctx.http, builder).await {
                        let existing_ids = Self::get_ids_from_messages(&existing_messages);
                        let mut embeds = Vec::new();
                        let mut embed_icon_files = Vec::new();
                        for item in news.iter() {
                            if !existing_ids.contains(&item.get_id()) && item.is_fresh(self.fresh_seconds) {
                                log_info("Sending news", Some(&format!("ID:{} Channel:{} Platforms:{:?}", item.get_id(), *channel_id, channel_platforms)));
                                let (summary, icon_files) = item.format_with_platforms(&channel_platforms);
                                let mut embed = CreateEmbed::default()
                                    .title(item.get_title())
                                    .url(&format!("https://playstartrekonline.com/en/news/article/{}", item.get_id()))
                                    .description(summary);
                                if let Some(img_url) = item.get_thumbnail_url() {
                                    embed = embed.thumbnail(img_url);
                                }
                                // Attach the first platform icon as the embed image (if any)
                                if let Some(icon_path) = icon_files.get(0) {
                                    let filename = icon_path.split('/').last().unwrap();
                                    embed = embed.image(format!("attachment://{}", filename));
                                    embed_icon_files.push(icon_path.clone());
                                } else {
                                    embed_icon_files.push(String::new());
                                }
                                embeds.push(embed);
                            }
                        }
                        if !embeds.is_empty() {
                            let mut msg = serenity::builder::CreateMessage::default().embeds(embeds);
                            for icon_path in embed_icon_files.iter().filter(|p| !p.is_empty()) {
                                if let Ok(attachment) = CreateAttachment::path(icon_path.clone()).await {
                                    msg = msg.add_file(attachment);
                                }
                            }
                            match channel.send_message(&ctx.http, msg).await {
                                Ok(_) => {},
                                Err(e) => log_error("Failed to send scheduled news message", e),
                            }
                        }
                    }
                }
            }
            time::sleep(Duration::from_secs(self.poll_period)).await;
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            if let Err(why) = self.handle_slash_command(&ctx, &command).await {
                log_error("Processing slash command interaction", why);
            }
        }
    }
}
