use std::collections::BTreeSet;
use std::fmt::Debug;
use std::slice::Iter;
use serde::Deserialize;
use serde_aux::prelude::*;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono::LocalResult::*;
use chrono_tz::America::Los_Angeles;
use chrono::Local; // Add this import for timestamps

#[derive(Deserialize, Clone)]
pub struct News {
    news: Vec<NewsItem>
}

impl News {
    pub fn filter_news_by_platform(&mut self, platforms: &BTreeSet<String>) -> bool{
        self.news.retain(|item| !platforms.is_disjoint(&item.platforms));
        if self.news.len() >= 1 {
            true
        }
        else {
            eprintln!("CEF:0|stobot|{}|{}|ERROR|No matching news|msg=No news item found matching the specified platforms: {:?}. | Context: Filtering news items by platform. end=epoch:{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), platforms, Local::now().timestamp_millis());
            false
        }
    }

    pub fn iter(&self) -> Iter<NewsItem> {
        self.news.iter()
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct NewsItem {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    id: u64,
    title: String,
    summary: String,
    platforms: BTreeSet<String>,
    updated: String,
    images: std::collections::HashMap<String, std::collections::HashMap<String, String>>
}

impl NewsItem {
    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn is_fresh(&self, diff_threshold: u64) -> bool {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&self.updated, "%Y-%m-%d %H:%M:%S") {
            if let Single(pacific) = Los_Angeles.from_local_datetime(&naive) {
                let diff = Utc::now().signed_duration_since(pacific);
                diff.num_seconds().abs() as u64 <= diff_threshold
            }
            else {
                false
            }
        }
        else {
            false
        }
    }

    pub fn is_within_weeks(&self, weeks: u32) -> bool {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&self.updated, "%Y-%m-%d %H:%M:%S") {
            if let Single(pacific) = Los_Angeles.from_local_datetime(&naive) {
                let diff = Utc::now().signed_duration_since(pacific);
                // Convert weeks to seconds for comparison (weeks * 7 days * 24 hours * 60 minutes * 60 seconds)
                diff.num_seconds().abs() as u64 <= (weeks as u64 * 7 * 24 * 60 * 60)
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn get_title(&self) -> &str {
        &self.title
    }

    pub fn get_thumbnail_url(&self) -> Option<&str> {
        self.images
            .get("img_microsite_thumbnail")
            .and_then(|img| img.get("url"))
            .map(|s| s.as_str())
    }

    pub fn get_tag(&self) -> &str {
        // Extract tag from URL or return an empty string
        // For example, if URL contains "/patch-notes/", return "patch-notes"
        if self.title.contains("Patch Notes") {
            "patch-notes"
        } else {
            "star-trek-online"
        }
    }

    pub fn format_with_platforms(&self, selected_platforms: &BTreeSet<String>) -> (String, Vec<String>) {
        let matching: Vec<&String> = self.platforms.iter().filter(|p| selected_platforms.contains(&p.to_lowercase())).collect();
        let icon_files: Vec<String> = matching.iter().map(|p| match p.to_lowercase().as_str() {
            "pc" => "static/pc.png".to_string(),
            "ps" | "playstation" => "static/ps.png".to_string(),
            "xbox" => "static/xbox.png".to_string(),
            _ => "static/unknown.png".to_string(),
        }).collect();
        (self.summary.clone(), icon_files)
    }
}

impl PartialEq for NewsItem{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
