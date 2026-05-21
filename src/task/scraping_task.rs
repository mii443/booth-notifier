use anyhow::Result;
use tracing::{debug, info};

use crate::{
    booth::item::{BoothDbClient, BoothItem},
    database::{DatabaseClient, NewFetchRun, NewItemSnapshot},
};

pub struct ScrapingTask {
    booth_db: BoothDbClient,
    last_run_item_ids: Vec<u64>,
}

impl ScrapingTask {
    pub fn new(booth_db: BoothDbClient) -> Self {
        Self {
            booth_db,
            last_run_item_ids: vec![],
        }
    }

    pub async fn run(&mut self, db: &DatabaseClient) -> Result<Vec<BoothItem>> {
        debug!("Starting scraping task");
        if self.last_run_item_ids.is_empty()
            && let Some(run) = db.get_latest_fetch_runs(1).await?.first()
        {
            self.last_run_item_ids = run.item_ids.iter().map(|id| *id as u64).collect();
        }

        let item_ids = self.booth_db.get_recent_item_ids().await?;
        let new_item_ids = self.calc_new_item_ids(&item_ids);

        db.create_fetch_run(NewFetchRun {
            item_ids: item_ids.iter().map(|id| *id as i64).collect(),
        })
        .await?;

        self.last_run_item_ids = item_ids;

        let mut items = vec![];
        for item_id in &new_item_ids {
            let item = self.booth_db.get_item(*item_id).await?;

            db.create_item_snapshot(NewItemSnapshot {
                item_id: *item_id as i64,
                name: item.name.clone(),
                payload: serde_json::to_value(&item)?,
            })
            .await?;

            info!("New item found: {} - {}", item.name, item.url);

            items.push(item);
        }

        Ok(items)
    }

    fn calc_new_item_ids(&self, item_ids: &[u64]) -> Vec<u64> {
        item_ids
            .iter()
            .filter(|id| !self.last_run_item_ids.contains(id))
            .copied()
            .collect()
    }
}
