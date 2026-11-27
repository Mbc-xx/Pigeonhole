//! The IMAP functions against the fake server: what they leave on the server
//! is what every other mail program of the user will see.

use super::fake_server::{email, start, Mailbox};
use super::*;

const ALL: &[&str] = &["MOVE", "UIDPLUS", "IDLE"];
const FOLDERS: &[&str] = &["INBOX", "Todo", "Archive"];

fn caps(r#move: bool, uidplus: bool) -> ServerCaps {
    ServerCaps {
        r#move,
        uidplus,
        ..Default::default()
    }
}

#[tokio::test]
async fn cards_come_with_their_flags_pinned_first_then_newest() {
    let mut mb = Mailbox::new(ALL, FOLDERS);
    mb.deliver("INBOX", &email("Old", "a@x", ""), &["\\Seen"], 1);
    mb.deliver("INBOX", &email("Pinned", "b@x", ""), &["\\Flagged"], 2);
    mb.deliver("INBOX", &email("New", "c@x", "In-Reply-To: <a@x>\n"), &[], 3);
    let (_server, mut s) = start(mb).await;

    let mut cache = FolderCache::default();
    let cards = load_cards(&mut s, "INBOX", &mut cache, 200).await.unwrap();
    let got: Vec<_> = cards.iter().map(|c| (c.subject.as_str(), c.flagged, c.seen)).collect();
    assert_eq!(
        got,
        [("Pinned", true, false), ("New", false, false), ("Old", false, true)]
    );
    assert_eq!(cards[1].thread_refs, ["a@x"]);
    assert_eq!(cards[2].date, 1_793_527_200, "1 Nov 2026, 10:00 UTC");
}

#[tokio::test]
async fn the_limit_keeps_the_newest_emails() {
    let mut mb = Mailbox::new(ALL, FOLDERS);
    for day in 1..=5 {
        mb.deliver(
            "INBOX",
            &email(&format!("Day {day}"), &format!("{day}@x"), ""),
            &[],
            day,
        );
    }
    let (_server, mut s) = start(mb).await;
    let cards = load_cards(&mut s, "INBOX", &mut FolderCache::default(), 2)
        .await
        .unwrap();
    let subjects: Vec<_> = cards.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["Day 5", "Day 4"]);
}

#[tokio::test]
async fn known_emails_are_not_downloaded_again_until_the_folder_is_renumbered() {
    let mut mb = Mailbox::new(ALL, FOLDERS);
    mb.deliver("INBOX", &email("One", "1@x", ""), &[], 1);
    let (server, mut s) = start(mb).await;
    let mut cache = FolderCache::default();
    load_cards(&mut s, "INBOX", &mut cache, 200).await.unwrap();
    assert_eq!(server.lock().unwrap().count("UID FETCH"), 1);

    // Read elsewhere: only the flags are asked again.
    server.lock().unwrap().folders.get_mut("INBOX").unwrap().mails[0]
        .flags
        .insert("\\Seen".into());
    let cards = load_cards(&mut s, "INBOX", &mut cache, 200).await.unwrap();
    assert!(cards[0].seen);
    assert_eq!(server.lock().unwrap().count("UID FETCH"), 1);

    // A new UIDVALIDITY means the old UIDs mean nothing any more.
    server.lock().unwrap().folders.get_mut("INBOX").unwrap().uid_validity = 2;
    load_cards(&mut s, "INBOX", &mut cache, 200).await.unwrap();
    assert_eq!(server.lock().unwrap().count("UID FETCH"), 2);
    assert_eq!(cache.uid_validity(), Some(2));
}

/// Moves emails 1 and 3 of four to Todo with the given server abilities.
async fn move_two(r#move: bool, uidplus: bool) -> std::sync::Arc<std::sync::Mutex<Mailbox>> {
    let advertised: &[&str] = match (r#move, uidplus) {
        (true, _) => ALL,
        (false, true) => &["UIDPLUS"],
        (false, false) => &[],
    };
    let mut mb = Mailbox::new(advertised, FOLDERS);
    for (i, flags) in [&["\\Flagged"][..], &[], &["\\Seen"], &[]].iter().enumerate() {
        mb.deliver(
            "INBOX",
            &email(&format!("Email {}", i + 1), &format!("{i}@x"), ""),
            flags,
            i as u32 + 1,
        );
    }
    let (server, mut s) = start(mb).await;
    move_messages(&mut s, caps(r#move, uidplus), "INBOX", &[1, 3], "Todo")
        .await
        .unwrap();
    {
        let mb = server.lock().unwrap();
        assert_eq!(mb.subjects("INBOX"), ["Email 2", "Email 4"]);
        assert_eq!(mb.subjects("Todo"), ["Email 1", "Email 3"]);
        assert_eq!(mb.flags("Todo", 1), ["\\Flagged"], "the pin travels with the email");
        assert_eq!(mb.flags("Todo", 2), ["\\Seen"]);
    }
    server
}

#[tokio::test]
async fn moving_uses_move_when_the_server_has_it() {
    let server = move_two(true, true).await;
    let mb = server.lock().unwrap();
    assert_eq!((mb.count("UID MOVE"), mb.count("UID COPY")), (1, 0));
}

#[tokio::test]
async fn moving_without_move_copies_then_removes_only_those_emails() {
    let server = move_two(false, true).await;
    let mb = server.lock().unwrap();
    assert_eq!(
        (mb.count("UID COPY"), mb.count("UID EXPUNGE"), mb.count("EXPUNGE")),
        (1, 1, 0)
    );
}

#[tokio::test]
async fn moving_on_a_basic_server_still_works() {
    let server = move_two(false, false).await;
    let mb = server.lock().unwrap();
    assert_eq!((mb.count("UID COPY"), mb.count("EXPUNGE")), (1, 1));
}

#[tokio::test]
async fn with_uidplus_a_move_leaves_other_deleted_emails_alone() {
    // Another program marked an email \Deleted without expunging it yet.
    let mut mb = Mailbox::new(&["UIDPLUS"], FOLDERS);
    mb.deliver("INBOX", &email("Mine", "1@x", ""), &[], 1);
    mb.deliver("INBOX", &email("Theirs", "2@x", ""), &["\\Deleted"], 2);
    let (server, mut s) = start(mb).await;
    move_messages(&mut s, caps(false, true), "INBOX", &[1], "Todo")
        .await
        .unwrap();
    assert_eq!(server.lock().unwrap().subjects("INBOX"), ["Theirs"]);
}

#[tokio::test]
async fn read_and_pin_change_on_the_server() {
    let mut mb = Mailbox::new(ALL, FOLDERS);
    mb.deliver("INBOX", &email("One", "1@x", ""), &[], 1);
    let (server, mut s) = start(mb).await;

    set_seen(&mut s, "INBOX", 1, true).await.unwrap();
    set_flagged(&mut s, "INBOX", 1, true).await.unwrap();
    assert_eq!(server.lock().unwrap().flags("INBOX", 1), ["\\Flagged", "\\Seen"]);
    set_seen(&mut s, "INBOX", 1, false).await.unwrap();
    assert_eq!(server.lock().unwrap().flags("INBOX", 1), ["\\Flagged"]);
    // The folder stays open between changes: one SELECT for all three.
    assert_eq!(server.lock().unwrap().count("SELECT"), 1);
}

#[tokio::test]
async fn every_copy_of_an_email_is_found_by_its_message_id() {
    // Sent to two addresses of the same mailbox: two copies, one Message-ID.
    let mut mb = Mailbox::new(ALL, FOLDERS);
    mb.deliver("INBOX", &email("Hello", "same@x", ""), &[], 1);
    mb.deliver("INBOX", &email("Other", "other@x", ""), &[], 2);
    mb.deliver("INBOX", &email("Hello", "same@x", ""), &[], 3);
    let (_server, mut s) = start(mb).await;
    s.open("INBOX").await.unwrap();
    assert_eq!(uids_by_message_id(&mut s, "<same@x>").await.unwrap(), [1, 3]);
    assert!(
        uids_by_message_id(&mut s, "bad\"id").await.is_err(),
        "no quotes in a search"
    );
}

#[tokio::test]
async fn a_missing_folder_is_an_error_not_a_panic() {
    let (_server, mut s) = start(Mailbox::new(ALL, FOLDERS)).await;
    let err = load_cards(&mut s, "Nope", &mut FolderCache::default(), 10)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot open folder Nope"), "{err}");
}

#[tokio::test]
async fn the_space_used_comes_from_the_quota_or_not_at_all() {
    let mut mb = Mailbox::new(&["QUOTA"], FOLDERS);
    mb.quota = (4_200_000, 10_485_760);
    let (_server, mut s) = start(mb).await;
    let got = space(&mut s).await.unwrap().unwrap();
    assert_eq!(got.used, 4_200_000 * 1024);
    assert_eq!(got.limit, 10 * 1024 * 1024 * 1024);

    // No limit set: nothing to show.
    let mut mb = Mailbox::new(&["QUOTA"], FOLDERS);
    mb.quota = (500, 0);
    let (_server, mut s) = start(mb).await;
    assert_eq!(space(&mut s).await.unwrap(), None);

    // A server without QUOTA is not asked.
    let (server, mut s) = start(Mailbox::new(ALL, FOLDERS)).await;
    assert_eq!(space(&mut s).await.unwrap(), None);
    assert_eq!(server.lock().unwrap().count("GETQUOTAROOT"), 0);
}

#[tokio::test]
async fn the_largest_emails_come_biggest_first_from_every_folder() {
    let big = |subject: &str, id: &str, kb: usize| email(subject, id, &format!("X-Pad: {}\n", "a".repeat(kb * 1024)));
    let mut mb = Mailbox::new(ALL, FOLDERS);
    mb.deliver("INBOX", &big("Medium", "m@x", 30), &[], 1);
    mb.deliver("INBOX", &email("Small", "s@x", ""), &[], 2);
    mb.deliver("Archive", &big("Huge", "h@x", 60), &[], 3);
    mb.deliver("Todo", &big("Just over", "j@x", 21), &[], 4);
    let (server, mut s) = start(mb).await;
    let q = SearchQuery {
        larger: Some(20 * 1024),
        ..Default::default()
    };
    let folders: Vec<String> = FOLDERS.iter().map(|f| f.to_string()).collect();
    let found = search(&mut s, &q, false, &folders, usize::MAX).await.unwrap();
    let got: Vec<_> = found
        .iter()
        .map(|e| (e.card.subject.as_str(), e.folder.as_str()))
        .collect();
    assert_eq!(got, [("Huge", "Archive"), ("Medium", "INBOX"), ("Just over", "Todo")]);
    assert!(found[0].card.size > 60 * 1024 && found[1].card.size > 30 * 1024);
    assert!(server
        .lock()
        .unwrap()
        .log
        .iter()
        .any(|l| l.ends_with("SEARCH CHARSET UTF-8 LARGER 20480")));
}

const SLIDES: &str = "From: Anna Bianchi <anna@example.com>\nTo: me@example.com\nSubject: Slides\nMessage-ID: <slides@example.com>\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=\"b1\"\n\n--b1\nContent-Type: text/plain\n\nHere are the slides.\n--b1\nContent-Type: application/pdf; name=\"slides.pdf\"\nContent-Disposition: attachment; filename=\"slides.pdf\"\nContent-Transfer-Encoding: base64\n\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\nJVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4K\n--b1--\n";

#[tokio::test]
async fn an_email_without_its_attachments_takes_its_place_and_the_original_waits_in_the_trash() {
    let mut mb = Mailbox::new(ALL, &["INBOX", "Archive", "Trash"]);
    mb.deliver("Archive", &email("Before", "b@x", ""), &[], 1);
    let uid = mb.deliver("Archive", SLIDES, &["\\Seen", "\\Flagged", "$Forwarded"], 5);
    let (server, mut s) = start(mb).await;
    let original = server.lock().unwrap().folders["Archive"].mails[1].raw.clone();
    let (lighter, saved) = crate::message::strip_attachments(&original, &[0], "27 Nov 2026").unwrap();
    assert!(saved > 0);
    replace_message(&mut s, caps(true, true), "Archive", uid, &lighter)
        .await
        .unwrap();

    let mb = server.lock().unwrap();
    let archive = &mb.folders["Archive"].mails;
    assert_eq!(archive.len(), 2, "same number of emails");
    let new = archive.last().unwrap();
    assert_eq!(new.raw, lighter);
    assert_eq!(
        new.flags,
        ["$Forwarded", "\\Flagged", "\\Seen"].map(String::from).into()
    );
    assert_eq!(new.internal, "05-Nov-2026 10:00:00 +0000", "the date stays");
    // The original, attachment included, can be taken back from the Trash.
    assert_eq!(mb.folders["Trash"].mails.len(), 1);
    assert_eq!(mb.folders["Trash"].mails[0].raw, original);
    // Added before anything is removed.
    let append = mb.log.iter().position(|l| l.starts_with("APPEND")).unwrap();
    let mv = mb.log.iter().position(|l| l.starts_with("UID MOVE")).unwrap();
    assert!(append < mv);
}

#[tokio::test]
async fn when_the_new_copy_cannot_be_saved_the_original_stays() {
    // The folder disappears: APPEND fails.
    let mut mb = Mailbox::new(ALL, &["INBOX", "Trash"]);
    let uid = mb.deliver("INBOX", SLIDES, &[], 5);
    let (server, mut s) = start(mb).await;
    let err = replace_message(&mut s, caps(true, true), "Gone", uid, b"x").await;
    assert!(err.is_err());
    let mb = server.lock().unwrap();
    assert_eq!(mb.folders["INBOX"].mails.len(), 1);
    assert!(mb.folders["Trash"].mails.is_empty());
    assert_eq!(mb.count("APPEND") + mb.count("UID MOVE"), 0);
}

#[tokio::test]
async fn in_the_trash_the_old_copy_is_simply_deleted() {
    let mut mb = Mailbox::new(ALL, &["INBOX", "Trash"]);
    let uid = mb.deliver("Trash", SLIDES, &["\\Seen"], 5);
    let (server, mut s) = start(mb).await;
    let original = server.lock().unwrap().folders["Trash"].mails[0].raw.clone();
    let (lighter, _) = crate::message::strip_attachments(&original, &[0], "27 Nov 2026").unwrap();
    replace_message(&mut s, caps(true, true), "Trash", uid, &lighter)
        .await
        .unwrap();
    let mb = server.lock().unwrap();
    let trash = &mb.folders["Trash"].mails;
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].raw, lighter);
}
