//! The artwork task: what it downloads, what it remembers, and outages.

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use cabinet_core::artwork::{self, ArtworkStore};
use cabinet_core::library::{self, NewRoot};
use cabinet_core::scan::ScanService;
use cabinet_core::tasks::{Kind, Scope, Services, Tasks, Trigger};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A stand-in for the provider's image server. Paths not in `images` are 404s.
#[derive(Clone)]
struct Images {
    url: String,
    images: Arc<Mutex<HashMap<String, &'static str>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Images {
    async fn start() -> Images {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = Images {
            url,
            images: Arc::default(),
            asked: Arc::default(),
        };
        let serving = server.clone();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 2048];
                let read = socket.read(&mut request).await.unwrap();
                let text = String::from_utf8_lossy(&request[..read]).to_string();
                let path = text.split_whitespace().nth(1).unwrap().to_string();
                serving.asked.lock().unwrap().push(path.clone());
                let image = serving.images.lock().unwrap().get(&path).copied();
                let response = match image {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    None => {
                        "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                            .to_string()
                    }
                };
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        server
    }

    fn add(&self, path: &str, body: &'static str) {
        self.images.lock().unwrap().insert(path.into(), body);
    }

    fn asked(&self) -> Vec<String> {
        let mut asked = self.asked.lock().unwrap().clone();
        asked.sort();
        asked
    }
}

struct Fixture {
    dir: TempDir,
    pool: SqlitePool,
    library: i64,
}

impl Fixture {
    async fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
            .await
            .unwrap();
        let library = library::create(&pool, "Shows", "tv").await.unwrap().id;
        let other = library::create(&pool, "Others", "tv").await.unwrap().id;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO item (id, kind, title, library_id) VALUES
                 (1, 'series', 'Show', {library}), (5, 'series', 'Elsewhere', {other});
             INSERT INTO series (item_id) VALUES (1), (5);
             INSERT INTO item (id, parent_id, library_id, kind, title)
                 VALUES (2, 1, {library}, 'episode', 'One');
             INSERT INTO item_artwork (item_id, kind, provider, path) VALUES
                 (1, 'poster', 'tmdb', '/p.jpg'),
                 (2, 'still', 'tmdb', '/s.jpg'),
                 (5, 'poster', 'tmdb', '/elsewhere.jpg');"
        )))
        .execute(&pool)
        .await
        .unwrap();
        Fixture { dir, pool, library }
    }

    fn store(&self, images: &str) -> ArtworkStore {
        ArtworkStore::with_images(self.dir.path().join("artwork"), images.into())
    }

    fn tasks(&self, store: &ArtworkStore) -> Tasks {
        Tasks::new(
            self.pool.clone(),
            Services {
                artwork: Some(store.clone()),
                ..Services::default()
            },
        )
    }

    async fn run(&self, tasks: &Tasks, trigger: Trigger) {
        let scope = Scope::Library(self.library);
        tasks.request(Kind::Artwork, scope, trigger).await.unwrap();
        tasks.wait_idle(Kind::Artwork, scope).await;
    }

    /// The latest artwork task: state, outcome, error.
    async fn last(&self) -> (String, serde_json::Value, Option<String>) {
        let (state, outcome, error): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT state, outcome, error FROM task WHERE kind = 'artwork' ORDER BY id DESC",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let outcome = outcome.map_or(serde_json::Value::Null, |o| {
            serde_json::from_str(&o).unwrap()
        });
        (state, outcome, error)
    }

    async fn remembered(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM artwork_unavailable")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn downloads_a_librarys_artwork_once_at_each_kinds_size() {
    let images = Images::start().await;
    images.add("/w500/p.jpg", "poster");
    images.add("/w780/s.jpg", "still");
    let f = Fixture::new().await;
    let store = f.store(&images.url);
    let tasks = f.tasks(&store);
    assert!(artwork::has_work(&f.pool, &store, f.library).await.unwrap());

    f.run(&tasks, Trigger::Added).await;

    assert_eq!(images.asked(), ["/w500/p.jpg", "/w780/s.jpg"]);
    let still = store.file("tmdb", "still", "/s.jpg").unwrap();
    assert_eq!(fs::read_to_string(&still).unwrap(), "still");
    let names: Vec<_> = fs::read_dir(still.parent().unwrap()).unwrap().collect();
    assert_eq!(names.len(), 1, "nothing temporary left behind");
    assert_eq!(f.last().await.1["saved"], 2);
    assert!(!artwork::has_work(&f.pool, &store, f.library).await.unwrap());

    // What's kept is served without asking the provider.
    f.run(&tasks, Trigger::Added).await;
    assert_eq!(
        store.get("tmdb", "still", "/s.jpg").await.unwrap(),
        b"still"
    );
    assert_eq!(images.asked().len(), 2);
}

#[tokio::test]
async fn what_the_provider_hasnt_got_is_remembered_until_asked_by_hand() {
    let images = Images::start().await;
    images.add("/w500/p.jpg", "poster");
    let f = Fixture::new().await;
    let store = f.store(&images.url);
    let tasks = f.tasks(&store);

    f.run(&tasks, Trigger::Added).await;
    let (state, outcome, error) = f.last().await;
    assert_eq!(state, "ok");
    assert_eq!(error, None);
    assert_eq!(outcome["unavailable"], 1);
    assert_eq!(outcome["failures"][0]["path"], "/s.jpg");
    assert_eq!(f.remembered().await, 1);
    assert!(!artwork::has_work(&f.pool, &store, f.library).await.unwrap());

    // Asked again in the background, it isn't requested.
    let before = images.asked().len();
    f.run(&tasks, Trigger::Added).await;
    assert_eq!(images.asked().len(), before);

    // By hand it is, and now the provider has it.
    images.add("/w780/s.jpg", "still");
    f.run(&tasks, Trigger::Manual).await;
    assert_eq!(f.last().await.1["saved"], 1);
    assert_eq!(f.remembered().await, 0);
}

#[tokio::test]
async fn an_outage_marks_nothing_and_only_fails_a_run_someone_asked_for() {
    // Nothing is listening here once the listener is gone.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let f = Fixture::new().await;
    let store = f.store(&url);
    let tasks = f.tasks(&store);

    f.run(&tasks, Trigger::Scheduled).await;
    let (state, outcome, error) = f.last().await;
    assert_eq!(state, "ok");
    assert_eq!(error, None);
    assert!(outcome["unreachable"].is_string());
    assert_eq!(f.remembered().await, 0);
    assert!(artwork::has_work(&f.pool, &store, f.library).await.unwrap());

    f.run(&tasks, Trigger::Manual).await;
    let (state, _, error) = f.last().await;
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("0 of 2"));
}

#[tokio::test]
async fn paths_that_arent_a_providers_are_refused() {
    let f = Fixture::new().await;
    let store = f.store("http://127.0.0.1:1");
    assert!(store.file("tmdb", "poster", "/../secret").is_none());
    assert!(store.file("tmdb", "poster", "no-slash.jpg").is_none());
    assert!(store.file("elsewhere", "poster", "/p.jpg").is_none());
}

#[tokio::test]
async fn a_scan_asks_for_what_its_library_is_missing() {
    let images = Images::start().await;
    images.add("/w500/p.jpg", "poster");
    images.add("/w780/s.jpg", "still");
    let f = Fixture::new().await;
    let media = f.dir.path().join("media");
    fs::create_dir_all(&media).unwrap();
    fs::write(media.join("a.mkv"), "x").unwrap();
    let root = library::add_root(
        &f.pool,
        f.library,
        NewRoot {
            path: media.to_str().unwrap(),
            write_policy: "read_only",
            layout_template: "{series}/{filename}",
            is_import_target: false,
        },
    )
    .await
    .unwrap()
    .id;
    let store = f.store(&images.url);
    let tasks = f.tasks(&store);

    let scans = ScanService::new(tasks.clone());
    scans.request(root, Trigger::Manual).await.unwrap();
    scans.wait_idle(root).await;
    tasks
        .wait_idle(Kind::Artwork, Scope::Library(f.library))
        .await;

    assert!(store.file("tmdb", "still", "/s.jpg").unwrap().exists());
    assert!(!images.asked().iter().any(|p| p.contains("elsewhere")));
}
