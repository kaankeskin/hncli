use std::{
    collections::{HashMap, hash_map::Entry},
    fs::{create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc, serde::ts_seconds};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::{
    api::types::HnItemIdScalar,
    config::get_project_os_directory,
    errors::{HnCliError, Result},
};

#[derive(Debug)]
pub enum HistoryPersistCommand {
    ResumeAdd {
        item_id: HnItemIdScalar,
        /// Human-readable title of the Item, displayed in the "resume Item reading" tab.
        label: String,
    },
    ResumeRemove {
        item_id: HnItemIdScalar,
    },
    TopLevelCommentAdd {
        story_id: HnItemIdScalar,
        top_level_comment_id: HnItemIdScalar,
    },
}

/// A piece of navigation state that can be stored in, and restored from, the history.
pub trait SynchronizedHistoryItem: Clone {
    /// Data stored alongside the navigation state ID, if any (`()` when there is none).
    type Metadata;

    /// Maximum number of entries that will be kept in the history file.
    fn max_entries() -> usize;
    /// Create an entry saving the given navigation state, stamped with the current datetime.
    fn created(id: HnItemIdScalar, metadata: Self::Metadata) -> Self;
    /// The datetime at which this `SynchronizedHistoryItem` was first inserted or last updated.
    fn get_timestamp(&self) -> &DateTime<Utc>;
    /// Get the stored ID corresponding to the saved navigation state.
    fn get_value(&self) -> HnItemIdScalar;
    /// Update the stored ID corresponding to the saved navigation state.
    fn set_value(&mut self, id: HnItemIdScalar, metadata: Self::Metadata);
}

/// Saves the "resume Item reading" navigation state.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ResumeItemHistoryData {
    #[serde(with = "ts_seconds")]
    datetime: DateTime<Utc>,
    /// Human-readable title of the Item, so that the "resume Item reading" tab
    /// can be displayed without fetching every stored Item again.
    label: String,
    item_id: HnItemIdScalar,
}

impl ResumeItemHistoryData {
    /// Get the stored, human-readable title of the Item.
    pub fn get_label(&self) -> &str {
        &self.label
    }
}

impl SynchronizedHistoryItem for ResumeItemHistoryData {
    type Metadata = String;

    fn max_entries() -> usize {
        100
    }
    fn created(id: HnItemIdScalar, metadata: Self::Metadata) -> Self {
        Self {
            datetime: Utc::now(),
            label: metadata,
            item_id: id,
        }
    }
    fn get_timestamp(&self) -> &DateTime<Utc> {
        &self.datetime
    }
    fn get_value(&self) -> HnItemIdScalar {
        self.item_id
    }
    fn set_value(&mut self, id: HnItemIdScalar, metadata: Self::Metadata) {
        self.datetime = Utc::now();
        self.label = metadata;
        self.item_id = id;
    }
}

/// Saves the navigation state of a top-level comment for a given Item thread.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct TopLevelCommentHistoryData {
    #[serde(with = "ts_seconds")]
    datetime: DateTime<Utc>,
    top_level_comment_id: HnItemIdScalar,
}

impl SynchronizedHistoryItem for TopLevelCommentHistoryData {
    type Metadata = ();

    fn max_entries() -> usize {
        500
    }
    fn created(id: HnItemIdScalar, _metadata: Self::Metadata) -> Self {
        Self {
            datetime: Utc::now(),
            top_level_comment_id: id,
        }
    }
    fn get_timestamp(&self) -> &DateTime<Utc> {
        &self.datetime
    }
    fn get_value(&self) -> HnItemIdScalar {
        self.top_level_comment_id
    }
    fn set_value(&mut self, id: HnItemIdScalar, _metadata: Self::Metadata) {
        self.datetime = Utc::now();
        self.top_level_comment_id = id;
    }
}

/// Storage for one kind of history entry, keyed by the related Hacker News item ID.
type SynchronizedHistoryItemStorage<T> = HashMap<HnItemIdScalar, T>;

#[derive(Debug, Deserialize, Serialize)]
pub struct SynchronizedHistory {
    /// Stores the information for the "resume Item reading" tab.
    ///
    /// When leaving an Item screen (eg. story or "Show HN")
    latest_resume_items_map: Option<SynchronizedHistoryItemStorage<ResumeItemHistoryData>>,
    /// Stores the latest focused top-level comment for a given Hacker News item.
    ///
    /// Also keeps track of the insertion datetime to enforce hard limits on the history size.
    latest_top_level_comments_per_item_map:
        Option<SynchronizedHistoryItemStorage<TopLevelCommentHistoryData>>,
}

impl SynchronizedHistory {
    fn empty() -> Self {
        Self {
            latest_resume_items_map: Some(SynchronizedHistoryItemStorage::with_capacity(
                ResumeItemHistoryData::max_entries(),
            )),
            latest_top_level_comments_per_item_map: Some(
                SynchronizedHistoryItemStorage::with_capacity(
                    TopLevelCommentHistoryData::max_entries(),
                ),
            ),
        }
    }

    /// Apply a history persist command to the in-memory history.
    fn apply(&mut self, command: &HistoryPersistCommand) {
        match command {
            // the "resume Item reading" storage is keyed by the very Item ID it stores
            HistoryPersistCommand::ResumeAdd { item_id, label } => Self::upsert_entry(
                self.latest_resume_items_map.get_or_insert_default(),
                *item_id,
                *item_id,
                label.clone(),
            ),
            HistoryPersistCommand::ResumeRemove { item_id } => {
                if let Some(storage) = self.latest_resume_items_map.as_mut() {
                    storage.remove(item_id);
                }
            }
            HistoryPersistCommand::TopLevelCommentAdd {
                story_id,
                top_level_comment_id,
            } => Self::upsert_entry(
                self.latest_top_level_comments_per_item_map
                    .get_or_insert_default(),
                *story_id,
                *top_level_comment_id,
                (),
            ),
        }
    }

    /// Store the given navigation state ID, and its associated metadata, in the storage,
    /// refreshing the already stored entry if there is one.
    fn upsert_entry<T: SynchronizedHistoryItem>(
        storage: &mut SynchronizedHistoryItemStorage<T>,
        key: HnItemIdScalar,
        value: HnItemIdScalar,
        metadata: T::Metadata,
    ) {
        match storage.entry(key) {
            Entry::Occupied(mut occupied) => occupied.get_mut().set_value(value, metadata),
            Entry::Vacant(vacant) => {
                vacant.insert(T::created(value, metadata));
            }
        }
    }

    /// Instantiate the synchronized history from the given JSON file.
    fn read_from_json_file(history_filepath: PathBuf) -> Self {
        // TODO: maybe a simple macro to reduce Result handling boilerplate
        // File existence/permissions check
        match history_filepath.try_exists().map_err(|err| {
            HnCliError::HistorySynchronizationError(format!(
                "cannot check if history file ({}) exists: {}",
                history_filepath.display(),
                err
            ))
        }) {
            Err(why) => {
                warn!("{why}");
                return Self::empty();
            }
            Ok(exists) => {
                if !exists {
                    warn!(
                        "history file ({}) does not exist yet",
                        history_filepath.display()
                    );
                    return Self::empty();
                }
            }
        }

        // Read
        let history_raw = match read_to_string(&history_filepath).map_err(|err| {
            HnCliError::HistorySynchronizationError(format!(
                "cannot open history file ({}): {}",
                history_filepath.display(),
                err
            ))
        }) {
            Ok(raw) => raw,
            Err(why) => {
                warn!("{why}");
                return Self::empty();
            }
        };

        // Deserialize
        let synchronized_history: Self = match serde_json::from_str(&history_raw).map_err(|err| {
            HnCliError::HistorySynchronizationError(format!("cannot deserialize history: {err}"))
        }) {
            Ok(read_history) => read_history,
            Err(why) => {
                warn!("{why}");
                return Self::empty();
            }
        };

        synchronized_history
    }

    /// Write the synchronized history to the given JSON file (erasing it if needed).
    ///
    /// This method should be not be called at every app interaction possible,
    /// for instance not at every top-level focused comment change.
    fn write_to_json_file<P: AsRef<Path>>(&self, history_filepath: P) -> Result<()> {
        let history_filepath = history_filepath.as_ref();
        let history_directory = history_filepath.parent().expect(
            "SynchronizedHistory.write_to_json_file: history filepath parent folder can be read",
        );
        create_dir_all(history_directory).map_err(|err| {
            HnCliError::HistorySynchronizationError(format!(
                "cannot create history directory ({:?}): {}",
                history_directory.display(),
                err
            ))
        })?;

        let limited_latest_resume_items_map =
            self.latest_resume_items_map.as_ref().map(|storage| {
                Self::enforced_history_limit(storage, ResumeItemHistoryData::max_entries())
            });
        let limited_latest_top_level_comments_per_item_map = self
            .latest_top_level_comments_per_item_map
            .as_ref()
            .map(|storage| {
                Self::enforced_history_limit(storage, TopLevelCommentHistoryData::max_entries())
            });
        let limited_synchronized_history = Self {
            latest_resume_items_map: limited_latest_resume_items_map,
            latest_top_level_comments_per_item_map: limited_latest_top_level_comments_per_item_map,
        };

        let history_raw = serde_json::to_string(&limited_synchronized_history).map_err(|err| {
            HnCliError::HistorySynchronizationError(format!("cannot serialize history: {err}"))
        })?;

        write(history_filepath, history_raw).map_err(|err| {
            HnCliError::HistorySynchronizationError(format!(
                "cannot save history file ({:?}): {}",
                history_filepath.display(),
                err
            ))
        })
    }

    /// Enforce an arbitrary items count limit on the stored navigation data.
    fn enforced_history_limit<T: SynchronizedHistoryItem>(
        storage: &SynchronizedHistoryItemStorage<T>,
        limit: usize,
    ) -> SynchronizedHistoryItemStorage<T> {
        Self::sorted_by_recency(storage)
            .into_iter()
            .take(limit)
            .map(|(id, storage_item)| (*id, storage_item.clone()))
            .collect()
    }

    /// Sort the stored navigation data, from the most recently updated entry to the oldest one.
    fn sorted_by_recency<T: SynchronizedHistoryItem>(
        storage: &SynchronizedHistoryItemStorage<T>,
    ) -> Vec<(&HnItemIdScalar, &T)> {
        let mut storage_entries: Vec<_> = storage.iter().collect();
        storage_entries.sort_by(|(_id_a, item_a), (_id_b, item_b)| {
            item_b.get_timestamp().cmp(item_a.get_timestamp())
        });
        storage_entries
    }
}

/// Responsible for restoring navigation state in the application from previous sessions.
#[derive(Debug)]
pub struct AppHistory {
    /// File-synchronized part of the navigation History.
    ///
    /// Reading must be done at application startup, and writing as rarely as possible.
    synchronized: SynchronizedHistory,
}

impl AppHistory {
    pub fn restored() -> Self {
        match Self::get_history_file_path() {
            Ok(history_file_path) => Self {
                synchronized: SynchronizedHistory::read_from_json_file(history_file_path),
            },
            Err(why) => {
                warn!(
                    "History: cannot retrieve OS filepath for history.json (reading history): {why}"
                );
                Self {
                    synchronized: SynchronizedHistory::empty(),
                }
            }
        }
    }

    /// Applies the given commands to the history and persist it
    /// in OS-dependent JSON storage.
    ///
    /// Should not be called too often for performance reasons.
    pub fn persist(&mut self, commands: &[HistoryPersistCommand]) {
        for command in commands {
            self.synchronized.apply(command);
        }

        match Self::get_history_file_path() {
            Ok(history_filepath) => {
                if let Err(why) = self.synchronized.write_to_json_file(history_filepath) {
                    warn!("History.persist error: {why}");
                }
            }
            Err(why) => {
                warn!(
                    "History: cannot retrieve OS filepath for history.json (writing history): {why}"
                );
            }
        }
    }

    /// Get the information needed for the "resume Item reading" tab,
    /// from the most recently visited Item to the oldest one.
    pub fn restored_resume_items(&self) -> Vec<&ResumeItemHistoryData> {
        self.synchronized
            .latest_resume_items_map
            .as_ref()
            .map_or_else(Vec::new, |storage| {
                SynchronizedHistory::sorted_by_recency(storage)
                    .into_iter()
                    .map(|(_item_id, resume_item)| resume_item)
                    .collect()
            })
    }

    pub fn restored_top_level_comment_id_for_story(
        &self,
        story_id: HnItemIdScalar,
    ) -> Option<HnItemIdScalar> {
        self.synchronized
            .latest_top_level_comments_per_item_map
            .as_ref()?
            .get(&story_id)
            .map(|entry| entry.get_value())
    }

    // TODO: add override possibility for integration tests
    fn get_history_file_path() -> Result<PathBuf> {
        let project_os_directory = get_project_os_directory()?;
        Ok(project_os_directory.join("history.json"))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::remove_dir_all;

    use chrono::Duration;

    use super::*;

    /// Apply the given commands to an in-memory history, without any persistence.
    fn applied_history(commands: &[HistoryPersistCommand]) -> AppHistory {
        let mut history = AppHistory {
            synchronized: SynchronizedHistory::empty(),
        };
        for command in commands {
            history.synchronized.apply(command);
        }
        history
    }

    /// Filepath of a real, test-owned history file, in a "/tmp" directory of its own.
    ///
    /// The directory is erased if a previous run left it behind.
    fn temporary_history_filepath(test_name: &str) -> PathBuf {
        let history_directory = PathBuf::from("/tmp").join(format!("hncli-history-{test_name}"));
        if history_directory.exists() {
            remove_dir_all(&history_directory)
                .expect("the temporary history directory can be erased");
        }
        history_directory.join("history.json")
    }

    /// A stable, human-readable label for the given Item, as stored by a `ResumeAdd` command.
    fn resume_label(item_id: HnItemIdScalar) -> String {
        format!("Story #{item_id}")
    }

    /// The stored navigation state IDs, sorted to be compared without any ordering assumption.
    fn sorted_stored_ids<T: SynchronizedHistoryItem>(
        storage: &Option<SynchronizedHistoryItemStorage<T>>,
    ) -> Vec<HnItemIdScalar> {
        let mut stored_ids: Vec<_> = storage
            .as_ref()
            .expect("the storage must be initialized")
            .values()
            .map(|entry| entry.get_value())
            .collect();
        stored_ids.sort_unstable();
        stored_ids
    }

    #[test]
    fn test_simple_item_top_level_persist_comment_id_scenario() {
        let history = applied_history(&[
            // viewed item 1 and left while focused on comment ID 123
            HistoryPersistCommand::TopLevelCommentAdd {
                story_id: 1,
                top_level_comment_id: 123,
            },
            // viewed item 2 and left while focused on comment ID 456
            HistoryPersistCommand::TopLevelCommentAdd {
                story_id: 2,
                top_level_comment_id: 456,
            },
            // viewed item 1 again, left while focused on comment ID 1230
            HistoryPersistCommand::TopLevelCommentAdd {
                story_id: 1,
                top_level_comment_id: 1230,
            },
        ]);

        // basic assertions
        assert_eq!(
            history.restored_top_level_comment_id_for_story(1),
            Some(1230)
        );
        assert_eq!(
            history.restored_top_level_comment_id_for_story(2),
            Some(456)
        );
        assert_eq!(history.restored_top_level_comment_id_for_story(3), None);
    }

    #[test]
    fn test_simple_resume_items_scenario() {
        let history = applied_history(&[
            HistoryPersistCommand::ResumeAdd {
                item_id: 1,
                label: resume_label(1),
            },
            HistoryPersistCommand::ResumeAdd {
                item_id: 2,
                label: resume_label(2),
            },
            // finished reading item 1
            HistoryPersistCommand::ResumeRemove { item_id: 1 },
            // removing an Item never added to the resume list is a no-op
            HistoryPersistCommand::ResumeRemove { item_id: 3 },
        ]);

        let resume_items = history.restored_resume_items();
        assert_eq!(resume_items.len(), 1);
        assert_eq!(resume_items.first().map(|entry| entry.get_value()), Some(2));
        // the label given at insertion is kept alongside the Item ID
        assert_eq!(
            resume_items.first().map(|entry| entry.get_label()),
            Some(resume_label(2).as_str())
        );
    }

    #[test]
    fn test_resume_items_restoring_order() {
        let mut history = applied_history(&[]);
        let resume_items_map = history
            .synchronized
            .latest_resume_items_map
            .get_or_insert_default();
        for (item_id, minutes_ago) in [(1, 5), (2, 37), (3, 1)] {
            resume_items_map.insert(
                item_id,
                ResumeItemHistoryData {
                    datetime: Utc::now() - Duration::minutes(minutes_ago),
                    label: resume_label(item_id),
                    item_id,
                },
            );
        }

        // from the most recently visited Item to the oldest one
        let restored_items_ids: Vec<_> = history
            .restored_resume_items()
            .iter()
            .map(|entry| entry.get_value())
            .collect();
        assert_eq!(restored_items_ids, vec![3, 1, 2]);

        // each label stays attached to its own Item through the sorting
        let restored_items_labels: Vec<_> = history
            .restored_resume_items()
            .iter()
            .map(|entry| entry.get_label().to_string())
            .collect();
        assert_eq!(
            restored_items_labels,
            vec![resume_label(3), resume_label(1), resume_label(2)]
        );
    }

    #[test]
    fn test_history_storage_limit_enforcing() {
        let mut storage = SynchronizedHistoryItemStorage::new();
        storage.insert(
            456,
            TopLevelCommentHistoryData {
                datetime: Utc::now(),
                top_level_comment_id: 4567,
            },
        );
        storage.insert(
            123,
            TopLevelCommentHistoryData {
                datetime: Utc::now() - Duration::minutes(3),
                top_level_comment_id: 1231,
            },
        );
        storage.insert(
            789,
            TopLevelCommentHistoryData {
                datetime: Utc::now() + Duration::seconds(37),
                top_level_comment_id: 7895,
            },
        );

        let limited_storage = SynchronizedHistory::enforced_history_limit(&storage, 2);

        assert_eq!(limited_storage.len(), 2);
        assert!(!limited_storage.contains_key(&123));
        assert!(limited_storage.contains_key(&456));
        assert!(limited_storage.contains_key(&789));
    }

    #[test]
    fn test_resume_item_upsert_refreshes_the_stored_entry() {
        let mut history = applied_history(&[
            HistoryPersistCommand::ResumeAdd {
                item_id: 1,
                label: resume_label(1),
            },
            HistoryPersistCommand::ResumeAdd {
                item_id: 2,
                label: resume_label(2),
            },
        ]);

        // backdate both entries to get a deterministic ordering
        for (item_id, minutes_ago) in [(1, 10), (2, 5)] {
            history
                .synchronized
                .latest_resume_items_map
                .get_or_insert_default()
                .entry(item_id)
                .and_modify(|entry| entry.datetime = Utc::now() - Duration::minutes(minutes_ago));
        }
        assert_eq!(
            history
                .restored_resume_items()
                .iter()
                .map(|entry| entry.get_value())
                .collect::<Vec<_>>(),
            vec![2, 1]
        );

        // visiting item 1 again refreshes its entry instead of duplicating it,
        // and stores the label as seen during this latest visit
        history
            .synchronized
            .apply(&HistoryPersistCommand::ResumeAdd {
                item_id: 1,
                label: "Story #1 (renamed)".into(),
            });

        let resume_items = history.restored_resume_items();
        assert_eq!(resume_items.len(), 2);
        assert_eq!(
            resume_items
                .iter()
                .map(|entry| entry.get_value())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            resume_items
                .iter()
                .map(|entry| entry.get_label())
                .collect::<Vec<_>>(),
            vec!["Story #1 (renamed)", resume_label(2).as_str()]
        );
    }

    #[test]
    fn test_history_json_file_write_then_read_round_trip() {
        let history_filepath = temporary_history_filepath("round-trip");
        let history = applied_history(&[
            HistoryPersistCommand::ResumeAdd {
                item_id: 1,
                label: resume_label(1),
            },
            HistoryPersistCommand::ResumeAdd {
                item_id: 2,
                label: resume_label(2),
            },
            HistoryPersistCommand::TopLevelCommentAdd {
                story_id: 1,
                top_level_comment_id: 123,
            },
        ]);

        // the parent directory does not exist yet, and must be created on the fly
        history
            .synchronized
            .write_to_json_file(&history_filepath)
            .expect("the history file can be written");
        assert!(history_filepath.exists());

        let restored = AppHistory {
            synchronized: SynchronizedHistory::read_from_json_file(history_filepath.clone()),
        };

        assert_eq!(
            sorted_stored_ids(&restored.synchronized.latest_resume_items_map),
            vec![1, 2]
        );
        assert_eq!(
            restored.restored_top_level_comment_id_for_story(1),
            Some(123)
        );
        assert_eq!(restored.restored_top_level_comment_id_for_story(2), None);

        assert_eq!(
            history.restored_resume_items()[0]
                .get_timestamp()
                .timestamp(),
            restored.restored_resume_items()[0]
                .get_timestamp()
                .timestamp()
        );

        // the labels survive the JSON round trip, still attached to their own Item
        let mut restored_labels: Vec<_> = restored
            .synchronized
            .latest_resume_items_map
            .as_ref()
            .expect("the resume items storage must be initialized")
            .iter()
            .map(|(item_id, entry)| (*item_id, entry.get_label().to_string()))
            .collect();
        restored_labels.sort_unstable();
        assert_eq!(
            restored_labels,
            vec![(1, resume_label(1)), (2, resume_label(2))]
        );

        remove_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be erased");
    }

    #[test]
    fn test_history_json_file_reading_when_missing() {
        let history_filepath = temporary_history_filepath("missing");

        let history = SynchronizedHistory::read_from_json_file(history_filepath);

        assert_eq!(
            sorted_stored_ids(&history.latest_resume_items_map),
            Vec::<HnItemIdScalar>::new()
        );
        assert_eq!(
            sorted_stored_ids(&history.latest_top_level_comments_per_item_map),
            Vec::<HnItemIdScalar>::new()
        );
    }

    #[test]
    fn test_history_json_file_reading_when_corrupted() {
        let history_filepath = temporary_history_filepath("corrupted");
        create_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be created");
        write(&history_filepath, "{ definitely not JSON ...")
            .expect("the corrupted history file can be written");

        // a corrupted history must not prevent the application from starting
        let history = SynchronizedHistory::read_from_json_file(history_filepath.clone());

        assert_eq!(
            sorted_stored_ids(&history.latest_resume_items_map),
            Vec::<HnItemIdScalar>::new()
        );

        remove_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be erased");
    }

    #[test]
    fn test_history_json_file_reading_without_the_resume_items_storage() {
        let history_filepath = temporary_history_filepath("no-resume-items");
        create_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be created");
        // history file written by a version of hncli predating the "resume Item reading" tab
        write(
            &history_filepath,
            r#"{"latest_top_level_comments_per_item_map":{"1":{"datetime":1750000000,"top_level_comment_id":123}}}"#,
        )
        .expect("the legacy history file can be written");

        let history = AppHistory {
            synchronized: SynchronizedHistory::read_from_json_file(history_filepath.clone()),
        };

        assert!(history.synchronized.latest_resume_items_map.is_none());
        assert!(history.restored_resume_items().is_empty());
        assert_eq!(
            history.restored_top_level_comment_id_for_story(1),
            Some(123)
        );

        remove_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be erased");
    }

    #[test]
    fn test_history_json_file_writing_enforces_the_entries_limit() {
        let history_filepath = temporary_history_filepath("entries-limit");
        let max_entries = ResumeItemHistoryData::max_entries();
        let mut history = applied_history(&[]);
        let resume_items_map = history
            .synchronized
            .latest_resume_items_map
            .get_or_insert_default();
        // the older the Item, the higher its ID here
        for item_id in 0..(max_entries + 20) as HnItemIdScalar {
            resume_items_map.insert(
                item_id,
                ResumeItemHistoryData {
                    datetime: Utc::now() - Duration::minutes(item_id.into()),
                    label: resume_label(item_id),
                    item_id,
                },
            );
        }

        history
            .synchronized
            .write_to_json_file(&history_filepath)
            .expect("the history file can be written");
        let restored = SynchronizedHistory::read_from_json_file(history_filepath.clone());

        let restored_items_ids = sorted_stored_ids(&restored.latest_resume_items_map);
        assert_eq!(restored_items_ids.len(), max_entries);
        // only the most recently visited Items are kept
        assert_eq!(
            restored_items_ids,
            (0..max_entries as HnItemIdScalar).collect::<Vec<_>>()
        );

        remove_dir_all(history_filepath.parent().unwrap())
            .expect("the temporary history directory can be erased");
    }
}
