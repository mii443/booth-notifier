use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{
    FromRow,
    postgres::{PgPool, PgPoolOptions},
    types::chrono::{DateTime, Utc},
};

#[derive(Clone)]
pub struct BoothDbClient {
    pool: PgPool,
    recent_item_limit: i64,
}

impl BoothDbClient {
    pub async fn new(database_url: &str, recent_item_limit: i64) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .context("Failed to connect to booth-db")?;

        Ok(Self {
            pool,
            recent_item_limit,
        })
    }

    pub async fn get_recent_item_ids(&self) -> Result<Vec<u64>> {
        let mut item_ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id
            FROM items
            WHERE COALESCE(is_sold_out, false) = false
              AND COALESCE(is_end_of_sale, false) = false
            ORDER BY published_at DESC, id DESC
            LIMIT $1
            "#,
        )
        .bind(self.recent_item_limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch recent item ids from booth-db")?;

        item_ids.reverse();
        item_ids
            .into_iter()
            .map(|id| {
                u64::try_from(id)
                    .with_context(|| format!("booth-db returned negative item id {id}"))
            })
            .collect()
    }

    pub async fn get_item(&self, id: u64) -> Result<BoothItem> {
        let id = i64::try_from(id).context("Item id is too large for booth-db")?;
        let row = sqlx::query_as::<_, BoothDbItemRow>(
            r#"
            SELECT
                i.id,
                i.url,
                i.title,
                i.price,
                i.shop_name,
                i.shop_url,
                i.shop_thumbnail_url,
                i.description,
                i.published_at,
                i.is_adult,
                i.is_sold_out,
                i.is_end_of_sale,
                i.wish_lists_count,
                i.wished,
                i.tags,
                c.id AS category_id,
                c.name AS category_name,
                c.url AS category_url,
                c.parent_name AS category_parent_name,
                c.parent_url AS category_parent_url
            FROM items i
            LEFT JOIN categories c ON c.id = i.category_id
            WHERE i.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Failed to fetch item {id} from booth-db"))?
        .with_context(|| format!("Item {id} was not found in booth-db"))?;

        let image_urls = sqlx::query_scalar::<_, String>(
            r#"
            SELECT url
            FROM item_images
            WHERE item_id = $1
            ORDER BY display_order ASC
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("Failed to fetch images for item {id} from booth-db"))?;

        let variation_rows = sqlx::query_as::<_, BoothDbVariationRow>(
            r#"
            SELECT json_variation_id, name, price, variation_type
            FROM item_variations
            WHERE item_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("Failed to fetch variations for item {id} from booth-db"))?;

        Ok(row.into_item(image_urls, variation_rows))
    }
}

#[derive(Debug, FromRow)]
struct BoothDbItemRow {
    id: i64,
    url: String,
    title: String,
    price: i64,
    shop_name: String,
    shop_url: String,
    shop_thumbnail_url: Option<String>,
    description: String,
    published_at: DateTime<Utc>,
    is_adult: Option<bool>,
    is_sold_out: Option<bool>,
    is_end_of_sale: Option<bool>,
    wish_lists_count: Option<i64>,
    wished: Option<bool>,
    tags: Option<Vec<String>>,
    category_id: Option<i64>,
    category_name: Option<String>,
    category_url: Option<String>,
    category_parent_name: Option<String>,
    category_parent_url: Option<String>,
}

impl BoothDbItemRow {
    fn into_item(
        self,
        image_urls: Vec<String>,
        variation_rows: Vec<BoothDbVariationRow>,
    ) -> BoothItem {
        let category = Category {
            id: self.category_id.unwrap_or_default() as u64,
            name: self.category_name.unwrap_or_default(),
            parent: self.category_parent_name.map(|name| CategoryParent {
                name,
                url: self.category_parent_url.unwrap_or_default(),
            }),
            url: self.category_url.unwrap_or_default(),
        };

        let tags = self
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(|name| Tag {
                url: String::new(),
                name,
            })
            .collect();

        let images = image_urls
            .into_iter()
            .map(|url| Image {
                caption: None,
                resized: url.clone(),
                original: url,
            })
            .collect();

        let variations = variation_rows
            .into_iter()
            .map(BoothDbVariationRow::into_variation)
            .collect();

        BoothItem {
            description: self.description,
            factory_description: None,
            id: self.id as u64,
            is_adult: self.is_adult.unwrap_or(false),
            is_buyee_possible: false,
            is_end_of_sale: self.is_end_of_sale.unwrap_or(false),
            is_placeholder: false,
            is_sold_out: self.is_sold_out.unwrap_or(false),
            name: self.title,
            published_at: self.published_at.to_rfc3339(),
            price: format!("JPY {}", self.price),
            purchase_limit: None,
            shipping_info: String::new(),
            small_stock: None,
            url: self.url,
            wish_list_url: String::new(),
            wish_lists_count: self.wish_lists_count.unwrap_or_default() as u64,
            wished: self.wished.unwrap_or(false),
            buyee_variations: vec![],
            category,
            embeds: vec![],
            images,
            order: None,
            gift: None,
            report_url: String::new(),
            share: Share::default(),
            shop: Shop {
                name: self.shop_name,
                subdomain: String::new(),
                thumbnail_url: self.shop_thumbnail_url.unwrap_or_default(),
                url: self.shop_url,
                verified: false,
            },
            sound: None,
            tags,
            tag_banners: vec![],
            tag_combination: None,
            tracks: None,
            variations,
        }
    }
}

#[derive(Debug, FromRow)]
struct BoothDbVariationRow {
    json_variation_id: Option<i64>,
    name: Option<String>,
    price: Option<i64>,
    variation_type: Option<String>,
}

impl BoothDbVariationRow {
    fn into_variation(self) -> Variation {
        Variation {
            buyee_html: None,
            downloadable: None,
            factory_image_url: None,
            has_download_code: false,
            id: self.json_variation_id.unwrap_or_default() as u64,
            is_anshin_booth_pack: false,
            is_empty_allocatable_stock_with_preorder: false,
            is_empty_stock: false,
            is_factory_item: false,
            is_mailbin: false,
            is_waiting_on_arrival: false,
            name: self.name,
            order_url: None,
            price: self.price.unwrap_or_default(),
            small_stock: None,
            status: String::new(),
            kind: match self.variation_type.as_deref() {
                Some("digital") => VariationType::Digital,
                _ => VariationType::Other,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BuyeeVariation {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Category {
    pub id: u64,
    pub name: String,
    pub parent: Option<CategoryParent>,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CategoryParent {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Image {
    pub caption: Option<String>,
    pub original: String,
    pub resized: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Share {
    #[serde(default)]
    pub hashtags: Vec<String>,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Shop {
    pub name: String,
    pub subdomain: String,
    pub thumbnail_url: String,
    pub url: String,
    pub verified: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Tag {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagBanner {
    pub image_url: Option<String>,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagCombination {
    pub category: String,
    pub tag: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Downloadable {
    Flag(bool),
    Detail(DownloadableDetail),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadableDetail {
    #[serde(default)]
    pub musics: Vec<DownloadFile>,
    #[serde(default)]
    pub no_musics: Vec<DownloadFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadFile {
    pub file_name: String,
    pub file_extension: String,
    pub file_size: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariationType {
    Digital,
    #[serde(other)]
    Other,
}

#[derive(Debug, Serialize, Deserialize)]
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
