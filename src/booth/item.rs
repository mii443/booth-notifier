use anyhow::{Context, Result};
use serde::Deserialize;

impl BoothItem {
    pub async fn from_id(id: u64) -> Result<Self> {
        let url = format!("https://booth.pm/ja/items/{id}.json");
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await?;
        let item: BoothItem = resp
            .json()
            .await
            .with_context(|| format!("Failed to parse JSON from {url}"))?;
        Ok(item)
    }
}

#[derive(Debug, Deserialize)]
pub struct BoothItem {
    pub description: String,
    pub factory_description: Option<String>,
    pub id: u64,
    pub is_adult: bool,
    pub is_buyee_possible: bool,
    pub is_end_of_sale: bool,
    pub is_placeholder: bool,
    pub is_sold_out: bool,
    pub name: String,
    pub published_at: String,
    pub price: String,
    pub purchase_limit: Option<u64>,
    pub shipping_info: String,
    pub small_stock: Option<i64>,
    pub url: String,
    pub wish_list_url: String,
    pub wish_lists_count: u64,
    pub wished: bool,

    #[serde(default)]
    pub buyee_variations: Vec<BuyeeVariation>,

    pub category: Category,

    #[serde(default)]
    pub embeds: Vec<Embed>,

    #[serde(default)]
    pub images: Vec<Image>,

    pub order: Option<serde_json::Value>,
    pub gift: Option<serde_json::Value>,
    pub report_url: String,
    pub share: Share,
    pub shop: Shop,
    pub sound: Option<serde_json::Value>,

    #[serde(default)]
    pub tags: Vec<Tag>,

    #[serde(default)]
    pub tag_banners: Vec<TagBanner>,

    pub tag_combination: Option<TagCombination>,
    pub tracks: Option<serde_json::Value>,

    #[serde(default)]
    pub variations: Vec<Variation>,
}

#[derive(Debug, Deserialize)]
pub struct BuyeeVariation {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct Category {
    pub id: u64,
    pub name: String,
    pub parent: Option<CategoryParent>,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct CategoryParent {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Embed {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct Image {
    pub caption: Option<String>,
    pub original: String,
    pub resized: String,
}

#[derive(Debug, Deserialize)]
pub struct Share {
    #[serde(default)]
    pub hashtags: Vec<String>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct Shop {
    pub name: String,
    pub subdomain: String,
    pub thumbnail_url: String,
    pub url: String,
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
pub struct Tag {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct TagBanner {
    pub image_url: Option<String>,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct TagCombination {
    pub category: String,
    pub tag: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariationType {
    Digital,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct Variation {
    pub buyee_html: Option<String>,
    pub downloadable: Option<bool>,
    pub factory_image_url: Option<String>,
    pub has_download_code: bool,
    pub id: u64,
    pub is_anshin_booth_pack: bool,
    pub is_empty_allocatable_stock_with_preorder: bool,
    pub is_empty_stock: bool,
    pub is_factory_item: bool,
    pub is_mailbin: bool,
    pub is_waiting_on_arrival: bool,
    pub name: Option<String>,
    pub order_url: Option<String>,
    pub price: i64,
    pub small_stock: Option<i64>,
    pub status: String,
    #[serde(rename = "type")]
    pub kind: VariationType,
}
