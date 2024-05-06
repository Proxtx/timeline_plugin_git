use {
    crate::{
        db::{Database, Event},
        PluginData,
    },
    chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc},
    futures::{StreamExt, TryStreamExt},
    git2::Repository,
    mongodb::{bson::doc, options::FindOptions},
    serde::{Deserialize, Serialize},
    std::{path::{Path, PathBuf}, thread},
    tokio::fs::read_dir,
    types::{
        api::{AvailablePlugins, CompressedEvent},
        timing::Timing,
    },
};

#[derive(Deserialize)]
struct ConfigData {
    pub repo_folder: PathBuf,
}

pub struct Plugin {
    plugin_data: PluginData,
    config: ConfigData,
}

impl crate::Plugin for Plugin {
    async fn new(data: PluginData) -> Self
    where
        Self: Sized,
    {
        let config: ConfigData = toml::Value::try_into(
            data.config
                .clone()
                .expect("Failed to init git plugin! No config was provided!"),
        )
        .unwrap_or_else(|e| {
            panic!(
                "Unable to init git plugin! Provided config does not fit the requirements: {}",
                e
            )
        });

        Plugin {
            plugin_data: data,
            config,
        }
    }

    fn get_type() -> types::api::AvailablePlugins
    where
        Self: Sized,
    {
        types::api::AvailablePlugins::timeline_plugin_git
    }

    fn request_loop<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = Option<chrono::Duration>> + Send + 'a>>
    {
        Box::pin(async move {
            if let Err(e) = self.update_commits().await {
                self.plugin_data
                    .report_error_string(format!("Unable to refresh recent commits: {}", e))
            }

            Some(Duration::try_minutes(15).unwrap())
        })
    }

    fn get_compressed_events(
        &self,
        query_range: &types::timing::TimeRange,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Future<Output = types::api::APIResult<Vec<types::api::CompressedEvent>>>
                + Send,
        >,
    > {
        let filter = Database::combine_documents(
            Database::generate_find_plugin_filter(Plugin::get_type()),
            Database::generate_range_filter(query_range)
        );
        let database = self.plugin_data.database.clone();
        Box::pin(async move {
            let mut cursor = database
                .get_events::<DatabaseCommit>()
                .find(filter, None)
                .await?;
            let mut result = Vec::new();
            while let Some(v) = cursor.next().await {
                let t = v?;
                result.push(CompressedEvent {
                    title: t.event.repository_name.clone(),
                    time: t.timing,
                    data: Box::new(t.event),
                })
            }

            Ok(result)
        })
    }
}

impl Plugin {
    async fn update_commits(&self) -> Result<(), String> {
        let mut dirs = match read_dir(&self.config.repo_folder).await {
            Ok(v) => v,
            Err(e) => return Err(format!("Unable to read repositories directory: {}", e)),
        };

        while let Some(entry) = match dirs.next_entry().await {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Unable to read repositories directory: {}", e));
            }
        } {
            let entry_type = match entry.file_type().await {
                Ok(v) => v,
                Err(e) => {
                    return Err(format!(
                        "Unable to check directory entry for file-type: {}",
                        e
                    ));
                }
            };

            if entry_type.is_dir() {
                self.get_commits_in_repo(&entry.path()).await?;
            }
        }

        Ok(())
    }

    async fn get_commits_in_repo(&self, path: &Path) -> Result<(), String> {
        let repo_name = match path.file_name() {
            Some(v) => match v.to_str() {
                Some(v) => v.to_string(),
                None => {
                    return Err("Unable to read os-string to internal string.".to_string());
                }
            },
            None => {
                return Err(
                    "Unable to identify repository name. There is no last path component.".to_string());
            }
        };

        let path = path.to_owned();

        let handle = thread::spawn(move || {
            let repo = match Repository::open(&path) {
                Ok(v) => v,
                Err(e) => {
                    return Err(format!(
                        "Unable to read repository: \nPath: {} \nError: {}",
                        path.display(),
                        e
                    ));
                }
            };

            let mut walk = match repo.revwalk() {
                Ok(v) => v,
                Err(e) => {
                    return Err(format!(
                        "Unable to read commits of repository: \nPath: {} \nError: {}",
                        path.display(),
                        e
                    ))
                }
            };

            match (walk.set_sorting(git2::Sort::TIME), walk.push_head()) {
                (Ok(_), Ok(_)) => {}
                (Err(e), Err(e_2)) => {
                    return Err(format!("Was unable to set sorting and append head to revwalk. \nPath: {} \nError Sorting: {} \nError Head: {}", path.display(), e, e_2));
                }
                (Err(e), _) => {
                    return Err(format!(
                        "Was unable to set sorting: \nPath: {} \nError: {}",
                        path.display(),
                        e
                    ));
                }
                (_, Err(e)) => {
                    return Err(format!(
                        "Was unable to append head: \nPath: {} \nError: {}",
                        path.display(),
                        e
                    ));
                }
            }

            let mut commits: Vec<Commit> = Vec::new();

            for step in walk {
                let step_id = match step {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(format!(
                            "Unable to walk along commit graph: \nPath: {} \nError: {}",
                            path.display(),
                            e
                        ));
                    }
                };

                let commit = match repo.find_commit(step_id) {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(format!("Unable to find a commit from revwalk in repository: \nPath: {} \nError: {}", path.display(), e));
                    }
                };

                let msg = match commit.message() {
                    Some(v) => v,
                    None => {
                        return Err(format!(
                            "Unable to read commit message: \nPath: {}",
                            path.display()
                        ));
                    }
                };

                let time = commit.time();

                let offset = FixedOffset::west_opt(time.offset_minutes() * 60).unwrap();
                let timestamp = offset.timestamp_millis_opt(time.seconds() * 1000).unwrap();
                let utc = DateTime::<Utc>::from(timestamp);

                let parsed_commit = Commit {
                    id: step_id.to_string(),
                    message: msg.to_string(),
                    time: utc,
                    repository_name: repo_name.clone(),
                    author: commit.author().to_string(),
                };

                commits.push(parsed_commit);
            }

            Ok(commits)
        });

        let commits = match handle.join() {
            Ok(v) => match v {
                Ok(v) => v,
                Err(e) => return Err(e)
            },
            Err(e) => {
                return Err(format!("Unable to join handle on update thread: {:?}", e))
            }
        };

        self.insert_new_commits_into_database(&commits).await
    }

    async fn insert_new_commits_into_database(&self, commits: &Vec<Commit>) -> Result<(), String> {
        let ids: Vec<&str> = commits.iter().map(|v| v.id.as_str()).collect();

        let already_inserted_commits: Vec<String> = match self
        .plugin_data
        .database
        .get_events::<DatabaseCommit>()
        .find(
            Database::combine_documents(
                Database::generate_find_plugin_filter(AvailablePlugins::timeline_plugin_git),
                    doc! {
                        "id": {
                            "$in": ids
                        }
                    },
                ),
                None,
            )
            .await
            {
                Ok(v) => match v.try_collect::<Vec<Event<DatabaseCommit>>>().await {
                    Ok(v) => v.into_iter().map(|v| v.id).collect(),
                    Err(e) => {
                        return Err(format!(
                            "Unable to collect all already existing commits: {}",
                            e
                        ));
                    }
                },
                Err(e) => {
                    return Err(format!("Error loading commit ids from database: {}", e));
                }
            };
            
        let mut insert = Vec::new();
            
        for commit in commits {
            if !already_inserted_commits.contains(&commit.id) {
                insert.push(CommitEvent {
                    timing: Timing::Instant(commit.time),
                    id: commit.id.clone(),
                    plugin: AvailablePlugins::timeline_plugin_git,
                    event: DatabaseCommit {
                        author: commit.author.clone(),
                        message: commit.message.clone(),
                        repository_name: commit.repository_name.clone(),
                    },
                })
            }
        }
        if !insert.is_empty() {
            if let Err(e) = self.plugin_data.database.register_events(&insert).await {
                return Err(format!("Unable to insert into Database: {}", e));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Commit {
    id: String,
    message: String,
    author: String,
    time: DateTime<Utc>,
    repository_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DatabaseCommit {
    message: String,
    author: String,
    repository_name: String,
}

type CommitEvent = Event<DatabaseCommit>;
