use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::{cookie::Jar, Client, Url};
use scraper::{Html, Selector};
use serde::Deserialize;

fn get_client(url: &Url) -> Client {
    let cookit_str = "adult=t;";
    let cookies = Arc::new(Jar::default());
    cookies.add_cookie_str(cookit_str, url);

    let client_builder = reqwest::Client::builder();
    let client: Client = client_builder.cookie_provider(cookies).build().unwrap();

    client
}

pub async fn get_recent_item_ids() -> Result<Vec<u64>> {
    let url = Url::parse(
        "https://booth.pm/ja/items?adult=include&in_stock=true&sort=new&tags%5B%5D=VRChat",
    )
    .unwrap();
    let client = get_client(&url);
    let response = client.get(url).send().await?.text().await.unwrap();
    let document = Html::parse_document(&response);

    let selector = Selector::parse("li.item-card.l-card[data-product-id]").unwrap();

    let elements = document.select(&selector);
    let mut products = vec![];
    for element in elements {
        products.push(
            element
                .value()
                .attr("data-product-id")
                .unwrap()
                .parse::<u64>()?,
        );
    }

    products.reverse();
    Ok(products)
}

impl BoothItem {
    pub async fn from_id(id: u64) -> Result<Self> {
        let url = format!("https://booth.pm/ja/items/{id}.json");
        let client = get_client(&Url::parse(&url).unwrap());
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .send()
            .await?;
        let text = resp.text().await?;
        let item: BoothItem = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse JSON for item ID {}: {}", id, text))?;
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
    pub embeds: Vec<serde_json::Value>,

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
#[serde(untagged)]
pub enum Downloadable {
    Flag(bool),
    Detail(DownloadableDetail),
}

#[derive(Debug, Deserialize)]
pub struct DownloadableDetail {
    #[serde(default)]
    pub musics: Vec<DownloadFile>,
    #[serde(default)]
    pub no_musics: Vec<DownloadFile>,
}

#[derive(Debug, Deserialize)]
pub struct DownloadFile {
    pub file_name: String,
    pub file_extension: String,
    pub file_size: String,
    pub name: String,
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
    pub downloadable: Option<Downloadable>,
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
