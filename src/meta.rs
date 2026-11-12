//! Game details for the game page: Steam store (description, developer, release, genres) and
//! ProtonDB (how well it runs on Linux). Neither needs a key. Cached in SQLite as JSON.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    /// Steam app the details came from.
    pub appid: Option<u32>,
    pub description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub release: Option<String>,
    pub genres: Vec<String>,
    /// ProtonDB tier: platinum, gold, silver, bronze, borked, native...
    pub protondb: Option<String>,
    pub linux_native: bool,
}

impl Meta {
    pub fn protondb_label(&self) -> Option<String> {
        if self.linux_native {
            return Some("Runs natively on Linux".into());
        }
        let t = self.protondb.as_deref()?;
        let mut c = t.chars();
        let tier = c.next()?.to_uppercase().collect::<String>() + c.as_str();
        Some(format!("ProtonDB {tier}"))
    }
}

/// Details for a Steam app. Non-Steam games get none rather than a guessed title match.
pub async fn fetch(http: reqwest::Client, appid: u32) -> Meta {
    let mut m = Meta { appid: Some(appid), ..Default::default() };
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=english");
    if let Some(d) = json(&http, &url).await.and_then(|v| v.get(appid.to_string())?.get("data").cloned()) {
        let s = |k: &str| d.get(k).and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
        let first = |k: &str| d.get(k)?.as_array()?.first()?.as_str().map(str::to_owned);
        m.description = s("short_description");
        m.developer = first("developers");
        m.publisher = first("publishers");
        m.release = d.get("release_date").and_then(|r| r.get("date")?.as_str()).map(str::to_owned).filter(|s| !s.is_empty());
        m.genres = d
            .get("genres")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("description")?.as_str().map(str::to_owned))
            .take(3)
            .collect();
        m.linux_native = d.get("platforms").and_then(|p| p.get("linux")?.as_bool()).unwrap_or(false);
    }
    let url = format!("https://www.protondb.com/api/v1/reports/summaries/{appid}.json");
    m.protondb = json(&http, &url).await.and_then(|v| v.get("tier")?.as_str().map(str::to_owned)).filter(|t| t != "pending");
    m
}

async fn json(http: &reqwest::Client, url: &str) -> Option<Value> {
    let r = http.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json().await.ok()
}

/// Store text carries HTML entities and the odd tag.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"").replace("&amp;", "&").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").trim().to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleans_html() {
        assert_eq!(super::clean("Fight &amp; <b>explore</b> &quot;Hallownest&quot;"), "Fight & explore \"Hallownest\"");
    }
}
