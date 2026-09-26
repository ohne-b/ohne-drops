use super::*;
use crate::{
    domain::{ChannelIdentity, Game, MAX_ESTIMATED_MINUTES},
    store::{CampaignArchive, History, HistoryFilter},
    twitch::tests::{campaign_json, gql_mock, http, session, validation},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn miner(server: &MockServer) -> (tempfile::TempDir, Mining, watch::Sender<Intent>, PubSub) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _commands) = App::open(dir.path().to_owned(), "").unwrap();
    let client = TwitchClient::new(Arc::new(http(server)), &session());
    let (intent, receiver) = watch::channel(Intent::default());
    let (events, reader) = mpsc::channel(256);
    let journal = Arc::new(Mutex::new(ClaimJournal::load(dir.path()).unwrap()));
    let pool = PubSub::start(client.clone(), events);
    let mut mining = Mining::new(app, client, journal, receiver, reader);
    mining.refresh = false;
    mining.next_refresh = Instant::now() + Duration::from_secs(1800);
    mining.campaigns =
        vec![Campaign::parse(&campaign_json("one"), &HashMap::new(), Utc::now()).unwrap()];
    mining.channels = vec![Channel {
        identity: ChannelIdentity {
            id: 10,
            login: "streamer".into(),
            name: "Streamer".into(),
        },
        game: Some(Game {
            id: 1,
            name: "Rust".into(),
            slug: "rust".into(),
            image_url: String::new(),
        }),
        broadcast_id: Some("stream1".into()),
        viewers: Some(100),
        drops_enabled: true,
        acl_based: false,
        beacon_url: None,
    }];
    (dir, mining, intent, pool)
}
async fn select(mining: &mut Mining) -> Settings {
    let settings = Settings {
        games_to_watch: vec!["Rust".into()],
        ..Settings::default()
    };
    mining.app.snapshot.write().await.settings.values = settings.clone();
    mining.reselect(&settings).await;
    settings
}
async fn finish_job(mining: &mut Mining, pool: &PubSub) {
    let completed = tokio::time::timeout(Duration::from_secs(5), mining.jobs.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    mining.busy.remove(&completed.kind);
    mining.complete(completed.job, pool).await.unwrap();
}

#[tokio::test]
async fn only_selected_games_send_beacons_and_inventory_io_does_not_block_watch_cadence() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/streamer"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"{{"beacon_url":"{}/track"}}"#, server.uri())),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/track"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let (_dir, mut miner, _intent, mut pool) = miner(&server).await;
    miner.schedule(&Settings::default()).await;
    assert!(miner.jobs.is_empty());
    let settings = select(&mut miner).await;
    assert_eq!(miner.watching, Some(10));
    miner.busy.insert(JobKind::Inventory);
    miner.schedule(&settings).await;
    assert!(miner.busy.contains(&JobKind::Watch));
    finish_job(&mut miner, &pool).await;
    assert!(miner.next_watch >= Instant::now() + Duration::from_secs(58));
    assert!(miner.poll_at.unwrap() >= Instant::now() + Duration::from_secs(19));
    miner.schedule(&settings).await;
    assert!(miner.jobs.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.iter().filter(|r| r.url.path() == "/track").count(),
        1
    );
    miner.app.snapshot.write().await.settings.values = Settings::default();
    miner.reselect(&Settings::default()).await;
    assert!(miner.watching.is_none());
    pool.close().await;
}

#[tokio::test]
async fn ignored_and_unselected_earned_rewards_are_claimed_once_and_archived_durably() {
    let server = MockServer::start().await;
    gql_mock(&server, |q| {
        assert_eq!(q["operationName"], "DropsPage_ClaimDropRewards");
        json!({"data":{"claimDropRewards":{"status":"ELIGIBLE_FOR_ALL"}}})
    })
    .await;
    let (dir, mut miner, _intent, mut pool) = miner(&server).await;
    let settings = Settings {
        drop_name_blacklist: vec!["reward".into()],
        ..Settings::default()
    };
    miner.campaigns[0].drops[0].claim_id = Some("instance".into());
    miner.campaigns[0].ends_at = Utc::now() - chrono::Duration::hours(1);
    miner.schedule(&settings).await;
    miner.schedule(&settings).await;
    assert_eq!(miner.jobs.len(), 1);
    finish_job(&mut miner, &pool).await;
    assert!(miner.campaigns[0].drops[0].claimed);
    miner.publish(&settings).await.unwrap();
    assert!(miner.app.snapshot.read().await.wanted_items.is_empty());
    let history = History::load(dir.path());
    assert_eq!(history.total(), 1);
    assert_eq!(
        history.entries(&HistoryFilter::default())[0].image_url,
        "https://static-cdn.jtvnw.net/hat.png"
    );
    assert!(
        ClaimJournal::load(dir.path())
            .unwrap()
            .pending(42)
            .is_empty()
    );
    assert!(CampaignArchive::load(dir.path()).merge(vec![], Utc::now())[0].finished);
    miner.schedule(&settings).await;
    assert!(miner.jobs.is_empty());
    pool.close().await;
}

#[tokio::test]
async fn claim_journal_recovers_after_history_write_failure_without_fabricating_claims() {
    let server = MockServer::start().await;
    gql_mock(
        &server,
        |_| json!({"data":{"claimDropRewards":{"status":"ELIGIBLE_FOR_ALL"}}}),
    )
    .await;
    let (dir, miner, _intent, mut pool) = miner(&server).await;
    let entry = miner.campaigns[0].history_entry(&miner.campaigns[0].drops[0], Utc::now());
    let file = dir.path().join("drop_history.json");
    std::fs::create_dir(&file).unwrap();
    assert_eq!(
        claim(
            &miner.app,
            &miner.client,
            &miner.journal,
            entry.clone(),
            "instance"
        )
        .await,
        Err(TwitchError::Storage)
    );
    assert_eq!(miner.app.history.lock().await.total(), 0);
    assert_eq!(ClaimJournal::load(dir.path()).unwrap().pending(42).len(), 1);
    std::fs::remove_dir(&file).unwrap();
    let mut miner = miner;
    miner.journal = Arc::new(Mutex::new(ClaimJournal::load(dir.path()).unwrap()));
    miner.recover_claims().await.unwrap();
    assert_eq!(miner.app.history.lock().await.total(), 0);
    miner.campaigns[0].drops[0].mark_claimed(Utc::now());
    miner.recover_claims().await.unwrap();
    miner.recover_claims().await.unwrap();
    assert_eq!(miner.app.history.lock().await.total(), 1);
    assert!(
        ClaimJournal::load(dir.path())
            .unwrap()
            .pending(42)
            .is_empty()
    );
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/gql")
            .count(),
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn journal_must_be_durable_before_remote_claim_and_failed_claims_are_not_history() {
    let server = MockServer::start().await;
    gql_mock(
        &server,
        |_| json!({"data":{"claimDropRewards":{"status":"NOT_ELIGIBLE"}}}),
    )
    .await;
    let (dir, miner, _intent, mut pool) = miner(&server).await;
    let entry = miner.campaigns[0].history_entry(&miner.campaigns[0].drops[0], Utc::now());
    let file = dir.path().join("pending_claims.json");
    std::fs::create_dir(&file).unwrap();
    assert_eq!(
        claim(
            &miner.app,
            &miner.client,
            &miner.journal,
            entry.clone(),
            "instance"
        )
        .await,
        Err(TwitchError::Storage)
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() != "/gql")
    );
    std::fs::remove_dir(&file).unwrap();
    assert!(
        !claim(&miner.app, &miner.client, &miner.journal, entry, "instance")
            .await
            .unwrap()
    );
    assert_eq!(miner.app.history.lock().await.total(), 0);
    assert!(
        ClaimJournal::load(dir.path())
            .unwrap()
            .pending(42)
            .is_empty()
    );
    pool.close().await;
}

#[tokio::test]
async fn progress_stays_confirmed_only_with_account_evidence_and_stalls_at_fifteen_estimates() {
    let server = MockServer::start().await;
    let (_dir, mut miner, _intent, mut pool) = miner(&server).await;
    let settings = select(&mut miner).await;
    let confirmed = miner.campaigns[0].drops[0].confirmed_at;
    for _ in 0..MAX_ESTIMATED_MINUTES {
        miner
            .complete(
                Job::Poll {
                    channel: 10,
                    result: Ok(None),
                },
                &pool,
            )
            .await
            .unwrap();
    }
    assert_eq!(miner.campaigns[0].drops[0].estimated_minutes, 15);
    assert_eq!(miner.campaigns[0].drops[0].confirmed_minutes, 12);
    assert_eq!(miner.campaigns[0].drops[0].confirmed_at, confirmed);
    miner.reselect(&settings).await;
    assert!(miner.watching.is_none());
    miner
        .event(Event::Progress {
            id: "drop-one".into(),
            minutes: 28,
        })
        .await
        .unwrap();
    assert_eq!(miner.campaigns[0].drops[0].estimated_minutes, 0);
    miner.reselect(&settings).await;
    assert_eq!(miner.watching, Some(10));
    pool.close().await;
}

#[tokio::test]
async fn inventory_refresh_cannot_replace_progress_confirmed_after_the_request_started() {
    let server = MockServer::start().await;
    let (_dir, mut miner, _intent, mut pool) = miner(&server).await;
    let before = Utc::now() - chrono::Duration::seconds(1);
    miner.confirm("drop-one", 31);
    let inventory = Inventory {
        campaigns: vec![
            Campaign::parse(&campaign_json("one"), &HashMap::new(), Utc::now()).unwrap(),
        ],
        status: InventoryStatus {
            available: true,
            ..InventoryStatus::default()
        },
    };
    miner
        .complete(
            Job::Inventory {
                result: Ok(inventory),
                requested_at: before,
            },
            &pool,
        )
        .await
        .unwrap();
    assert_eq!(miner.campaigns[0].drops[0].confirmed_minutes, 31);
    miner.publish(&Settings::default()).await.unwrap();
    assert_eq!(
        miner.app.snapshot.read().await.campaigns[0].drops[0].confirmed_minutes,
        31
    );
    pool.close().await;
}

#[tokio::test]
async fn transitions_manual_failover_cache_clear_and_refresh_setting_preserve_user_data() {
    let server = MockServer::start().await;
    let (dir, mut miner, intent, mut pool) = miner(&server).await;
    let settings = select(&mut miner).await;
    session().save(dir.path()).unwrap();
    miner.app.data.save_settings(&settings).unwrap();
    let mut alternate = miner.channels[0].clone();
    alternate.identity.id = 11;
    miner.channels.push(alternate);
    intent.send_modify(|v| {
        v.selected = Some(10);
        v.manual_revision += 1;
    });
    miner.apply_intent(&pool).await;
    assert_eq!(miner.manual, Some((10, 1)));
    miner.event(Event::Offline(10)).await.unwrap();
    miner.reselect(&settings).await;
    assert_eq!(miner.watching, Some(11));
    assert!(miner.manual.is_some());
    miner.event(Event::Changed(10)).await.unwrap();
    assert!(miner.refresh_channels[&10] >= Instant::now() + Duration::from_secs(119));
    miner.campaigns[0].drops[0].ends_at = Utc::now() + chrono::Duration::seconds(10);
    miner.set_transition();
    assert!(miner.next_transition.unwrap() <= Utc::now() + chrono::Duration::seconds(10));
    miner
        .app
        .snapshot
        .write()
        .await
        .settings
        .values
        .minimum_refresh_interval_minutes = 1;
    intent.send_modify(|v| v.settings += 1);
    miner.apply_intent(&pool).await;
    assert_eq!(
        miner.next_refresh,
        miner.last_inventory + Duration::from_secs(60)
    );
    let epoch = miner.epoch;
    intent.send_modify(|v| {
        v.clear += 1;
        v.refresh += 1
    });
    miner.apply_intent(&pool).await;
    assert!(miner.refresh);
    assert!(miner.channels.is_empty());
    assert!(miner.campaigns.is_empty());
    assert!(miner.manual.is_none());
    assert_ne!(miner.epoch, epoch);
    assert_eq!(Session::load(dir.path()).unwrap().unwrap().user_id, 42);
    assert_eq!(
        miner.app.data.settings().unwrap().games_to_watch,
        vec!["Rust"]
    );
    pool.close().await;
}

#[tokio::test]
async fn completed_archives_and_subscription_only_campaigns_never_become_live_mining_inventory() {
    let server = MockServer::start().await;
    let (_dir, mut miner, _intent, mut pool) = miner(&server).await;
    miner.campaigns[0].drops[0].mark_claimed(Utc::now());
    miner.publish(&Settings::default()).await.unwrap();
    miner.campaigns.clear();
    let mut subscription = campaign_json("sub");
    subscription["timeBasedDrops"][0]["requiredMinutesWatched"] = 0.into();
    miner
        .campaigns
        .push(Campaign::parse(&subscription, &HashMap::new(), Utc::now()).unwrap());
    let settings = select(&mut miner).await;
    miner.publish(&settings).await.unwrap();
    let state = miner.app.snapshot.read().await;
    assert_eq!(state.campaigns.len(), 1);
    assert!(state.campaigns[0].finished);
    assert!(state.wanted_items.is_empty());
    assert!(state.current_drop.is_none());
    assert!(miner.watching.is_none());
    pool.close().await;
}

#[tokio::test]
async fn concurrent_logout_and_shutdown_drain_owned_work_before_removing_only_twitch_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let (app, commands) = App::open(dir.path().to_owned(), "").unwrap();
    session().save(dir.path()).unwrap();
    std::fs::write(dir.path().join("cookies.jar"), b"rollback copy").unwrap();
    let settings = Settings::default()
        .patched(&json!({"games_to_watch":["Rust"]}))
        .unwrap();
    app.data.save_settings(&settings).unwrap();
    let campaign = Campaign::parse(&campaign_json("history"), &HashMap::new(), Utc::now()).unwrap();
    app.history
        .lock()
        .await
        .record(campaign.history_entry(&campaign.drops[0], Utc::now()))
        .unwrap();
    let mut owner = Miner::new(app.clone(), commands);
    let cancel = CancellationToken::new();
    let stopped = cancel.clone();
    let complete = Arc::new(Notify::new());
    let released = complete.clone();
    let (intent, _) = watch::channel(Intent::default());
    let (drained, drained_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        stopped.cancelled().await;
        released.notified().await;
        let _ = drained.send(());
        Ok(())
    });
    let generation = Generation {
        cancel,
        confirmed: Arc::new(Notify::new()),
        intent,
        task,
    };
    let (first, first_result) = oneshot::channel();
    let logout = tokio::spawn(async move {
        owner.logout(generation, first).await;
    });
    let second_app = app.clone();
    let second = tokio::spawn(async move { second_app.command(Command::Logout).await });
    let shutdown_app = app.clone();
    let shutdown = tokio::spawn(async move { shutdown_app.command(Command::Shutdown).await });
    tokio::time::timeout(Duration::from_secs(3), app.shutdown.cancelled())
        .await
        .unwrap();
    assert!(Session::load(dir.path()).unwrap().is_some());
    complete.notify_one();
    drained_rx.await.unwrap();
    first_result.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    shutdown.await.unwrap().unwrap();
    logout.await.unwrap();
    assert!(Session::load(dir.path()).unwrap().is_none());
    assert_eq!(
        std::fs::read(dir.path().join("cookies.jar")).unwrap(),
        b"rollback copy"
    );
    assert_eq!(History::load(dir.path()).total(), 1);
    assert_eq!(app.data.settings().unwrap().games_to_watch, vec!["Rust"]);
}

#[tokio::test]
async fn full_owner_authenticates_saved_session_and_logout_cancels_inventory_before_new_device_flow()
 {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(validation()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tv"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/oauth2/device")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"device_code":"private","user_code":"NEWCODE","verification_uri":"https://www.twitch.tv/activate","interval":1,"expires_in":60}))).mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/gql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(30))
                .set_body_json(json!({"data":{"currentUser":{"inventory":{}}}})),
        )
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (app, commands) = App::open(dir.path().to_owned(), "").unwrap();
    session().save(dir.path()).unwrap();
    let mut miner = Miner::new(app.clone(), commands);
    miner.endpoints = Endpoints::mock(&server.uri());
    let worker = tokio::spawn(miner.run());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.url.path() == "/gql")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), app.command(Command::Logout))
        .await
        .unwrap()
        .unwrap();
    assert!(Session::load(dir.path()).unwrap().is_none());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app
                .snapshot
                .read()
                .await
                .login
                .oauth_pending
                .as_ref()
                .is_some_and(|v| v.code == "NEWCODE")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(app.snapshot.read().await.campaigns.is_empty());
    app.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(Session::load(dir.path()).unwrap().is_none());
}
