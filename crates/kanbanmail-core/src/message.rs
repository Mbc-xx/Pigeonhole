//! Message (MIME) parsing for cards and the reader.

use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};

/// A board card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub uid: u32,
    pub from: String,
    pub from_address: String,
    pub subject: String,
    /// Seconds since 1970 (UTC), 0 if unknown.
    pub date: i64,
    pub preview: String,
    pub flagged: bool,
    pub seen: bool,
    /// The email seems to carry attachments (best guess from its first bytes).
    pub has_attachments: bool,
    /// Message-ID header, used to group conversations.
    pub message_id: Option<String>,
    /// Sent by a machine (newsletter, notification, mailing list, auto-reply).
    pub automated: bool,
    /// Message-IDs this email refers to (In-Reply-To and References: the thread
    /// root and the most recent ones), used to group conversations in lists.
    pub thread_refs: Vec<String>,
    /// To and Cc addresses, comma-separated (used by the automatic rules).
    pub to: String,
    /// Only the To addresses, comma-separated.
    #[serde(default)]
    pub to_only: String,
    /// Size on the server in bytes, attachments included; 0 when not asked for.
    #[serde(default)]
    pub size: u32,
    /// Set on a copy whose attachments this app removed (its X-Pigeonhole-Removed
    /// header): it shares its Message-ID with the original, but not its content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_mark: Option<String>,
}

/// An attachment that can be downloaded from the reader.
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentInfo {
    /// Position among the message attachments, used to download it.
    pub index: usize,
    pub name: String,
    pub mime: String,
    pub size: usize,
}

/// An attachment removed by this app: "slides.pdf, 1.2 MB" and the day.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemovedAttachment {
    pub what: String,
    pub day: String,
}

/// The header that marks a copy whose attachments were removed, with a value
/// unique to each removal.
pub const REMOVED_HEADER: &str = "X-Pigeonhole-Removed";

/// How the note left in place of a removed attachment starts.
const REMOVED_NOTE: &str = "[Attachment removed by Pigeonhole on ";

/// The notes of removed attachments, among the text parts of the email.
fn removed_attachments(m: &mail_parser::Message<'_>) -> Vec<RemovedAttachment> {
    m.text_body()
        .chain(m.attachments())
        .filter(|p| matches!(p.kind(), mail_parser::PartKind::Text))
        .filter_map(|p| {
            let text = String::from_utf8_lossy(&p.decoded()).into_owned();
            let rest = text.trim().strip_prefix(REMOVED_NOTE)?.strip_suffix(']')?.to_string();
            let (day, what) = rest.split_once(": ")?;
            Some(RemovedAttachment {
                what: what.to_string(),
                day: day.to_string(),
            })
        })
        .collect()
}

/// The full content of an email, for the reader.
#[derive(Debug, Clone, Serialize)]
pub struct MessageView {
    pub from: String,
    /// Address to reply to (Reply-To if present, otherwise From).
    pub reply_address: String,
    pub to: String,
    /// Addresses in To and Cc, for "Reply all".
    pub all_recipients: Vec<String>,
    pub cc: String,
    /// Only present in your own copies (Sent folder, drafts).
    pub bcc: String,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub date: i64,
    pub text: Option<String>,
    /// Sanitized HTML: no scripts or forms. Remote images are kept in the
    /// markup but only load if the reader allows them (see the UI).
    pub html: Option<String>,
    /// The HTML references remote images or other remote resources.
    pub has_remote_content: bool,
    /// The HTML without the quoted earlier messages, when a quote was found.
    pub html_main: Option<String>,
    /// The text without the quoted earlier messages, when a quote was found.
    pub text_main: Option<String>,
    /// Attachments to download (inline images shown in the HTML are left out).
    pub attachments: Vec<AttachmentInfo>,
    /// A calendar invitation carried by the email, if any.
    pub invite: Option<Invite>,
    /// How a draft of this app was written ("rich", "markdown", "plain").
    pub draft_format: Option<String>,
    /// How to leave the mailing list the email comes from, if it says.
    pub unsubscribe: Option<Unsubscribe>,
    /// Attachments taken out by this app to save space: the notes left in their place.
    pub removed: Vec<RemovedAttachment>,
}

/// The List-Unsubscribe header (RFC 2369) and its one-click form (RFC 8058).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Unsubscribe {
    /// A web address to leave the list.
    pub url: Option<String>,
    /// An address to write to instead, with its subject (a `mailto:` link).
    pub mailto: Option<String>,
    /// The web address takes a plain POST and needs no page to be opened.
    pub one_click: bool,
}

/// Reads List-Unsubscribe: `<https://…>, <mailto:…>`. Only web and mail links.
pub fn parse_unsubscribe(header: &str, post: Option<&str>) -> Option<Unsubscribe> {
    let mut url = None;
    let mut mailto = None;
    for part in header.split(',') {
        let link = part.trim().trim_start_matches('<').trim_end_matches('>').trim();
        let lower = link.to_ascii_lowercase();
        if url.is_none() && (lower.starts_with("https://") || lower.starts_with("http://")) {
            url = Some(link.to_string());
        } else if mailto.is_none() && lower.starts_with("mailto:") {
            mailto = Some(link.to_string());
        }
    }
    let one_click = url
        .as_deref()
        .is_some_and(|u| u.to_ascii_lowercase().starts_with("https://"))
        && post.is_some_and(|p| p.to_ascii_lowercase().contains("list-unsubscribe=one-click"));
    (url.is_some() || mailto.is_some()).then_some(Unsubscribe { url, mailto, one_click })
}

/// A date-time from a calendar file: local wall time unless `utc`.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct IcsTime {
    /// "2026-10-28"
    pub date: String,
    /// "10:00", absent for all-day events.
    pub time: Option<String>,
    pub utc: bool,
}

/// The main event of a calendar invitation (text/calendar part).
#[derive(Debug, Clone, Serialize, Default)]
pub struct Invite {
    /// REQUEST (invitation), CANCEL, REPLY...
    pub method: String,
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub start: IcsTime,
    pub end: Option<IcsTime>,
    pub organizer: String,
    pub organizer_name: String,
    /// The whole calendar file, to answer or add it to a calendar.
    pub ics: String,
}

fn sender(list: Option<mail_parser::AddressList<'_>>) -> String {
    list.and_then(|l| l.first())
        .map(|m| match (m.name(), m.address()) {
            (Some(n), _) if !n.is_empty() => n.to_string(),
            (_, Some(a)) => a.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn recipients(list: Option<mail_parser::AddressList<'_>>) -> String {
    list.map(|l| {
        l.mailboxes()
            .filter_map(|m| m.address().or(m.name()).map(str::to_string))
            .collect::<Vec<_>>()
            .join(", ")
    })
    .unwrap_or_default()
}

/// Builds a card from the first bytes of a message (may be truncated).
pub fn parse_card(uid: u32, raw: &[u8], flagged: bool, seen: bool, fallback_date: i64) -> Card {
    let parsed = MessageParser::default().parse(raw);
    let from_address = parsed
        .as_ref()
        .and_then(|m| m.from())
        .and_then(|l| l.first())
        .and_then(|m| m.address().map(str::to_string))
        .unwrap_or_default();
    let removed_mark = parsed.as_ref().and_then(|m| {
        let value = m.headers().get(REMOVED_HEADER)?;
        let mark: String = String::from_utf8_lossy(value.raw_value())
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
            .take(40)
            .collect();
        (!mark.is_empty()).then_some(mark)
    });
    let has_attachments = parsed.as_ref().is_some_and(|m| {
        // Once attachments were removed, only those left count: the email
        // stays "mixed", with notes where the files were.
        (removed_mark.is_none()
            && m.content_type().is_some_and(|ct| {
                ct.ctype().eq_ignore_ascii_case("multipart")
                    && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
            }))
            || m.attachments().any(|p| p.content_id().is_none())
    });
    let (from, subject, date, preview) = match &parsed {
        Some(m) => {
            let preview = m
                .body_preview(400)
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            (
                sender(m.from()),
                m.subject().unwrap_or_default().to_string(),
                m.date().map(|d| d.to_timestamp()).unwrap_or(fallback_date),
                preview.chars().take(200).collect(),
            )
        }
        None => (String::new(), String::new(), fallback_date, String::new()),
    };
    let message_id = parsed.as_ref().and_then(|m| m.message_id()).map(str::to_string);
    let automated = parsed.as_ref().is_some_and(|m| is_automated(m, &from_address));
    let thread_refs = parsed.as_ref().map(thread_refs).unwrap_or_default();
    let to = parsed
        .as_ref()
        .map(|m| {
            [m.to(), m.cc()]
                .into_iter()
                .flatten()
                .flat_map(|l| {
                    l.mailboxes()
                        .filter_map(|mb| mb.address().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let to_only = parsed
        .as_ref()
        .and_then(|m| m.to())
        .map(|l| {
            l.mailboxes()
                .filter_map(|mb| mb.address().map(str::to_string))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    Card {
        uid,
        from,
        from_address,
        subject,
        date,
        preview,
        flagged,
        seen,
        has_attachments,
        message_id,
        automated,
        thread_refs,
        to,
        to_only,
        size: 0,
        removed_mark,
    }
}

/// In-Reply-To plus the first and the last few References (enough to link a
/// message to its thread without carrying very long reference chains).
fn thread_refs(m: &mail_parser::Message<'_>) -> Vec<String> {
    let mut refs: Vec<String> = m
        .references()
        .map(|r| (0..r.len()).filter_map(|i| r.get(i)).map(str::to_string).collect())
        .unwrap_or_default();
    if refs.len() > 6 {
        let tail = refs.split_off(refs.len() - 5);
        refs.truncate(1);
        refs.extend(tail);
    }
    if let Some(r) = m.in_reply_to().and_then(|r| r.get(0)) {
        if !refs.iter().any(|x| x == r) {
            refs.push(r.to_string());
        }
    }
    refs
}

/// Local parts used by machines rather than people.
const MACHINE_SENDERS: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "do_not_reply",
    "notification",
    "notifications",
    "notify",
    "mailer-daemon",
    "postmaster",
    "bounce",
    "bounces",
    "alert",
    "alerts",
    "newsletter",
    "newsletters",
    "automated",
    "automailer",
    "system",
    "daemon",
];

/// True for emails written by a machine: mailing lists and newsletters
/// (List-Id / List-Unsubscribe), bulk or automatic messages (Precedence,
/// Auto-Submitted) and typical "noreply"-style senders.
pub fn is_automated(m: &mail_parser::Message<'_>, from_address: &str) -> bool {
    let headers = m.headers();
    let value = |name: &str| {
        headers
            .get(name)
            .map(|h| String::from_utf8_lossy(h.raw_value()).trim().to_ascii_lowercase())
    };
    if headers.get("List-Unsubscribe").is_some() || headers.get("List-Id").is_some() {
        return true;
    }
    if value("Precedence").is_some_and(|p| ["bulk", "list", "junk", "auto_reply"].contains(&p.as_str())) {
        return true;
    }
    if value("Auto-Submitted").is_some_and(|a| a != "no") {
        return true;
    }
    if headers.get("X-Autoreply").is_some() || headers.get("X-Autorespond").is_some() {
        return true;
    }
    let local = from_address.split('@').next().unwrap_or("").to_ascii_lowercase();
    MACHINE_SENDERS.iter().any(|m| {
        local == *m
            || local.starts_with(&format!("{m}+"))
            || local.starts_with(&format!("{m}."))
            || local.starts_with(&format!("{m}-"))
            || local.ends_with(&format!("-{m}"))
            || local.ends_with(&format!(".{m}"))
    })
}

pub fn parse_view(raw: &[u8]) -> MessageView {
    let Some(m) = MessageParser::default().parse(raw) else {
        return MessageView {
            from: String::new(),
            reply_address: String::new(),
            to: String::new(),
            all_recipients: Vec::new(),
            cc: String::new(),
            bcc: String::new(),
            message_id: None,
            in_reply_to: None,
            references: Vec::new(),
            subject: "(unreadable message)".into(),
            date: 0,
            text: Some(String::from_utf8_lossy(raw).into_owned()),
            html: None,
            has_remote_content: false,
            invite: None,
            html_main: None,
            text_main: None,
            attachments: Vec::new(),
            draft_format: None,
            unsubscribe: None,
            removed: Vec::new(),
        };
    };
    // Use HTML only if the message really has some: otherwise mail-parser
    // generates it from the text, and we prefer showing plain text.
    let has_real_html = m.html_body().any(|p| matches!(p.kind(), mail_parser::PartKind::Html));
    let raw_html = if has_real_html {
        m.body_html(0).map(|h| h.into_owned())
    } else {
        None
    };
    let html = raw_html.as_deref().map(|h| sanitize_html(&inline_cid_images(&m, h)));
    let html_main = raw_html
        .as_deref()
        .and_then(|h| quote_start_html(h).map(|pos| sanitize_html(&inline_cid_images(&m, &h[..pos]))))
        .filter(|main| !html_to_plain(main).trim().is_empty());
    let text = m.body_text(0).map(|t| t.into_owned());
    let text_main = text
        .as_deref()
        .and_then(|t| quote_start_text(t).map(|pos| t[..pos].trim_end().to_string()))
        .filter(|main| !main.trim().is_empty());
    let first_address = |l: Option<mail_parser::AddressList<'_>>| {
        l.and_then(|l| l.first()).and_then(|m| m.address().map(str::to_string))
    };
    let addresses = |l: Option<mail_parser::AddressList<'_>>| -> Vec<String> {
        l.map(|l| l.mailboxes().filter_map(|m| m.address().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    let mut all_recipients = addresses(m.to());
    all_recipients.extend(addresses(m.cc()));
    let mut references: Vec<String> = m
        .references()
        .map(|r| (0..r.len()).filter_map(|i| r.get(i)).map(str::to_string).collect())
        .unwrap_or_default();
    let message_id = m.message_id().map(str::to_string);
    if references.is_empty() {
        if let Some(r) = m.in_reply_to() {
            references.extend((0..r.len()).filter_map(|i| r.get(i)).map(str::to_string));
        }
    }
    let header = |name: &str| {
        m.headers().get(name).map(|h| {
            String::from_utf8_lossy(h.raw_value())
                .replace(['\r', '\n'], " ")
                .trim()
                .to_string()
        })
    };
    let unsubscribe =
        header("List-Unsubscribe").and_then(|h| parse_unsubscribe(&h, header("List-Unsubscribe-Post").as_deref()));
    MessageView {
        unsubscribe,
        removed: removed_attachments(&m),
        from: sender(m.from()),
        reply_address: first_address(m.reply_to())
            .or_else(|| first_address(m.from()))
            .unwrap_or_default(),
        to: recipients(m.to()),
        all_recipients,
        cc: recipients(m.cc()),
        bcc: recipients(m.bcc()),
        message_id,
        in_reply_to: m.in_reply_to().and_then(|r| r.get(0)).map(str::to_string),
        references,
        subject: m.subject().unwrap_or_default().to_string(),
        date: m.date().map(|d| d.to_timestamp()).unwrap_or(0),
        text,
        text_main,
        html_main,
        has_remote_content: html.as_deref().is_some_and(has_remote_content),
        attachments: list_attachments(&m, raw_html.as_deref()),
        invite: find_invite(&m),
        draft_format: header(crate::smtp::DRAFT_FORMAT_HEADER),
        html,
    }
}

fn mime_of(part: &mail_parser::MessagePart<'_>) -> String {
    part.content_type()
        .map(|ct| match ct.subtype() {
            Some(sub) => format!("{}/{}", ct.ctype(), sub),
            None => ct.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into())
        .to_lowercase()
}

/// Inline images referenced by the HTML (`cid:`) become `data:` URIs, so they
/// show without any network access.
fn inline_cid_images(m: &mail_parser::Message<'_>, html: &str) -> String {
    use base64::Engine;
    let mut out = html.to_string();
    for part in m.attachments() {
        let (Some(cid), true) = (part.content_id(), mime_of(&part).starts_with("image/")) else {
            continue;
        };
        let needle = format!("cid:{cid}");
        if out.contains(&needle) {
            let data = base64::engine::general_purpose::STANDARD.encode(part.decoded());
            out = out.replace(&needle, &format!("data:{};base64,{data}", mime_of(&part)));
        }
    }
    out
}

fn list_attachments(m: &mail_parser::Message<'_>, raw_html: Option<&str>) -> Vec<AttachmentInfo> {
    m.attachments()
        .enumerate()
        .filter(|(_, p)| {
            // Images already shown inside the HTML are not listed.
            !matches!((p.content_id(), raw_html), (Some(cid), Some(h)) if h.contains(&format!("cid:{cid}")))
        })
        .map(|(index, p)| AttachmentInfo {
            index,
            name: p
                .attachment_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("attachment-{}", index + 1)),
            mime: mime_of(&p),
            size: p.decoded_len(),
        })
        .collect()
}

/// Name, MIME type and content of the `index`-th attachment.
pub fn attachment_with_mime(raw: &[u8], index: usize) -> Option<(String, String, Vec<u8>)> {
    let m = MessageParser::default().parse(raw)?;
    let part = m.attachments().nth(index)?;
    let name = part
        .attachment_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("attachment-{}", index + 1));
    Some((name, mime_of(&part), part.decoded().into_owned()))
}

/// Name and content of the `index`-th attachment.
pub fn attachment(raw: &[u8], index: usize) -> Option<(String, Vec<u8>)> {
    let m = MessageParser::default().parse(raw)?;
    let part = m.attachments().nth(index)?;
    let name = part
        .attachment_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("attachment-{}", index + 1));
    Some((name, part.decoded().into_owned()))
}

/// "4.2 MB", "320 KB", for the note left where an attachment was.
fn human_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{} KB", b.div_ceil(1024)),
        b => format!("{:.1} MB", b as f64 / 1024.0 / 1024.0),
    }
}

/// The email with the chosen attachments (by their `index`, as listed in the
/// reader) replaced by a short note that names them, said to be removed on
/// `day`. Everything else, headers included, stays byte for byte, so the
/// email keeps its Message-ID, its conversation and its text. Signed or
/// encrypted emails are refused: changing them would break the signature.
/// Returns the new email and the bytes saved.
pub fn strip_attachments(raw: &[u8], indexes: &[usize], day: &str) -> Result<(Vec<u8>, usize), String> {
    use base64::Engine;
    let m = MessageParser::default().parse(raw).ok_or("the email cannot be read")?;
    let top = m
        .content_type()
        .map(|ct| format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("")).to_lowercase())
        .unwrap_or_default();
    if matches!(
        top.as_str(),
        "multipart/signed" | "multipart/encrypted" | "application/pkcs7-mime" | "application/x-pkcs7-mime"
    ) {
        return Err("This email is signed or encrypted: removing attachments would break it.".into());
    }
    let nl: &[u8] = if raw.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    let base = raw.as_ptr() as usize;
    let mut cuts: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    for (index, part) in m.attachments().enumerate() {
        if !indexes.contains(&index) {
            continue;
        }
        let (start, end) = (part.offset_header() as usize, part.offset_end() as usize);
        // Only a part written as such in this email can be cut out: not the
        // whole email, nor one inside a forwarded email.
        let here = part.raw().as_ptr() as usize == base + start && end <= raw.len() && start < end;
        if start == 0 || !here {
            return Err(
                "This attachment cannot be removed (it is inside a forwarded email or is the whole email).".into(),
            );
        }
        let name: String = part
            .attachment_name()
            .unwrap_or("attachment")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let size = part.decoded_len();
        let note = format!("{REMOVED_NOTE}{day}: {name}, {}]", human_size(size));
        let encoded = base64::engine::general_purpose::STANDARD.encode(note.as_bytes());
        let mut new = Vec::new();
        for line in [
            "Content-Type: text/plain; charset=utf-8",
            "Content-Disposition: inline",
            "Content-Transfer-Encoding: base64",
            "",
        ] {
            new.extend_from_slice(line.as_bytes());
            new.extend_from_slice(nl);
        }
        for (i, chunk) in encoded.as_bytes().chunks(76).enumerate() {
            if i > 0 {
                new.extend_from_slice(nl);
            }
            new.extend_from_slice(chunk);
        }
        cuts.push((start, end, new));
    }
    if cuts.len() != indexes.iter().collect::<std::collections::BTreeSet<_>>().len() {
        return Err("Some of the attachments are no longer in the email.".into());
    }
    cuts.sort_by_key(|c| c.0);
    if cuts.windows(2).any(|w| w[1].0 < w[0].1) {
        return Err("These attachments overlap: remove them one at a time.".into());
    }
    let mut out = raw.to_vec();
    for (start, end, new) in cuts.into_iter().rev() {
        out.splice(start..end, new);
    }
    // The mark goes first, so the card of the lighter copy (read from the
    // first bytes) knows it is not the original.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}.{:09}", d.as_secs(), d.subsec_nanos()))
        .unwrap_or_else(|_| "0".into());
    let mut mark = format!("{REMOVED_HEADER}: {stamp}").into_bytes();
    mark.extend_from_slice(nl);
    out.splice(0..0, mark);
    let saved = raw.len().saturating_sub(out.len());
    Ok((out, saved))
}

fn has_remote_content(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    [
        "src=\"http",
        "src='http",
        "url(http",
        "url(\"http",
        "url('http",
        "background=\"http",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

/// Cleans a signature written or pasted in the settings editor.
pub fn sanitize_signature(html: &str) -> String {
    ammonia::Builder::default()
        .url_schemes(["data", "mailto", "http", "https"].into_iter().collect())
        // data-signature marks the signature block in drafts written in the editor.
        .add_generic_attributes(["style", "align", "width", "height", "data-signature"])
        .link_rel(Some("noopener noreferrer"))
        .clean(html)
        .to_string()
}

/// Strips scripts, forms and iframes. Remote image addresses are kept: whether
/// they load is decided by the reader's Content-Security-Policy.
fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .url_schemes(["data", "mailto", "http", "https", "cid"].into_iter().collect())
        .add_generic_attributes(["style", "align", "bgcolor", "width", "height", "data-signature"])
        .link_rel(Some("noopener noreferrer"))
        .clean(html)
        .to_string()
}

/// Rough visible text of some HTML (only used to check a part is not empty).
fn html_to_plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
}

/// Where the quoted earlier messages start in an HTML body, as written by the
/// most common mail clients (Gmail, Apple Mail, Outlook, Thunderbird, Yahoo).
pub fn quote_start_html(html: &str) -> Option<usize> {
    // ASCII lowercasing keeps byte offsets unchanged.
    let lower = html.to_ascii_lowercase();
    let markers = [
        "<div class=\"gmail_quote",
        "<blockquote type=\"cite\"",
        "<div id=\"divrplyfwdmsg\"",
        "<div id=\"appendonsend\"",
        "<hr id=\"stopspelling\"",
        "<div class=\"moz-cite-prefix\"",
        "<div class=\"yahoo_quoted\"",
        "<blockquote class=\"gmail_quote\"",
    ];
    let mut best = markers.iter().filter_map(|m| lower.find(m)).min();
    // Outlook desktop: a reply header box with a thin top border.
    for border in ["border-top:solid #e1e1e1 1.0pt", "border-top:solid #b5c4df 1.0pt"] {
        if let Some(pos) = lower.find(border) {
            let start = lower[..pos].rfind("<div").unwrap_or(pos);
            best = Some(best.map_or(start, |b| b.min(start)));
        }
    }
    // Apple Mail and others write "On ..., X wrote:" right before the quote.
    if let Some(b) = best {
        if let Some(attr) = lower[..b].rfind("<div") {
            let between = html_to_plain(&html[attr..b]);
            let t = between.trim().to_lowercase();
            if t.ends_with("wrote:")
                || t.ends_with("ha scritto:")
                || t.ends_with("a écrit :")
                || t.ends_with("schrieb:")
            {
                best = Some(attr);
            }
        }
    }
    best.filter(|&b| b > 0)
}

/// Where the quoted earlier messages start in a plain-text body.
pub fn quote_start_text(text: &str) -> Option<usize> {
    let mut offset = 0;
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let lower = t.to_lowercase();
        let next = lines.get(i + 1).map(|l| l.trim().to_lowercase()).unwrap_or_default();
        let header_block = (lower.starts_with("from:") || lower.starts_with("da:") || lower.starts_with("de:"))
            && (next.starts_with("sent:")
                || next.starts_with("date:")
                || next.starts_with("inviato:")
                || next.starts_with("data:")
                || next.starts_with("to:")
                || next.starts_with("a:"));
        let wrote = (lower.starts_with("on ") && lower.ends_with("wrote:"))
            || ((lower.starts_with("il ") || lower.starts_with("in data ")) && lower.ends_with("ha scritto:"))
            || lower.ends_with("a écrit :")
            || lower.ends_with("schrieb:");
        let separator = lower.contains("original message") || lower.contains("messaggio originale");
        // A run of "> " lines that lasts until the end of the message.
        let quote_tail = t.starts_with('>')
            && lines[i..]
                .iter()
                .all(|l| l.trim().is_empty() || l.trim_start().starts_with('>'));
        if offset > 0 && (header_block || wrote || separator || quote_tail) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// The first calendar part (text/calendar or a .ics attachment) as an invitation.
fn find_invite(m: &mail_parser::Message<'_>) -> Option<Invite> {
    m.parts().find_map(|p| {
        let is_ics = mime_of(&p).starts_with("text/calendar")
            || p.attachment_name()
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
        if !is_ics {
            return None;
        }
        parse_ics(&String::from_utf8_lossy(&p.decoded()))
    })
}

/// Unfolded lines of a calendar file (continuation lines start with a space or tab).
fn ics_lines(ics: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in ics.split('\n').map(|l| l.trim_end_matches('\r')) {
        match (
            line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')),
            out.last_mut(),
        ) {
            (Some(rest), Some(prev)) => prev.push_str(rest),
            _ => out.push(line.to_string()),
        }
    }
    out
}

/// Splits "NAME;PARAM=x:value" into (NAME, params, value).
pub fn ics_property(line: &str) -> Option<(String, String, String)> {
    let colon = {
        // The value starts at the first colon outside quoted parameter values.
        let mut quoted = false;
        line.char_indices()
            .find(|&(_, c)| {
                if c == '"' {
                    quoted = !quoted;
                }
                c == ':' && !quoted
            })?
            .0
    };
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    Some((name.to_ascii_uppercase(), params.to_string(), value.to_string()))
}

fn ics_text(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn ics_param(params: &str, name: &str) -> Option<String> {
    params.split(';').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.eq_ignore_ascii_case(name).then(|| v.trim_matches('"').to_string())
    })
}

fn ics_time(params: &str, value: &str) -> IcsTime {
    let v = value.trim();
    let date = |d: &str| {
        if d.len() >= 8 {
            format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
        } else {
            d.to_string()
        }
    };
    if params.to_ascii_uppercase().contains("VALUE=DATE") && !v.contains('T') || v.len() == 8 {
        return IcsTime {
            date: date(v),
            time: None,
            utc: false,
        };
    }
    let (d, t) = v.split_once('T').unwrap_or((v, ""));
    let time = (t.len() >= 4).then(|| format!("{}:{}", &t[0..2], &t[2..4]));
    IcsTime {
        date: date(d),
        time,
        utc: t.ends_with('Z'),
    }
}

/// The first event of a calendar file.
pub fn parse_ics(ics: &str) -> Option<Invite> {
    let mut inv = Invite {
        ics: ics.to_string(),
        ..Default::default()
    };
    let mut in_event = false;
    let mut seen_event = false;
    for line in ics_lines(ics) {
        let Some((name, params, value)) = ics_property(&line) else {
            continue;
        };
        match (name.as_str(), in_event) {
            ("METHOD", false) => inv.method = value.trim().to_ascii_uppercase(),
            ("BEGIN", _) if value.eq_ignore_ascii_case("VEVENT") && !seen_event => in_event = true,
            ("END", true) if value.eq_ignore_ascii_case("VEVENT") => {
                in_event = false;
                seen_event = true;
            }
            ("UID", true) => inv.uid = value,
            ("SUMMARY", true) => inv.summary = ics_text(&value),
            ("LOCATION", true) => inv.location = ics_text(&value),
            ("DTSTART", true) => inv.start = ics_time(&params, &value),
            ("DTEND", true) => inv.end = Some(ics_time(&params, &value)),
            ("ORGANIZER", true) => {
                inv.organizer = value
                    .trim_start_matches("mailto:")
                    .trim_start_matches("MAILTO:")
                    .to_string();
                inv.organizer_name = ics_param(&params, "CN").unwrap_or_default();
            }
            _ => {}
        }
    }
    (seen_event && !inv.uid.is_empty()).then_some(inv)
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_unsubscribe_header_is_read() {
        let u = parse_unsubscribe(
            "<mailto:leave@list.example?subject=unsubscribe>, <https://list.example/u/1>",
            Some("List-Unsubscribe=One-Click"),
        )
        .unwrap();
        assert_eq!(u.url.as_deref(), Some("https://list.example/u/1"));
        assert_eq!(
            u.mailto.as_deref(),
            Some("mailto:leave@list.example?subject=unsubscribe")
        );
        assert!(u.one_click);
        let plain = parse_unsubscribe("<http://list.example/u>", Some("List-Unsubscribe=One-Click")).unwrap();
        assert!(!plain.one_click, "one click only over https");
        assert_eq!(parse_unsubscribe("<javascript:alert(1)>, <file:///etc>", None), None);
    }

    #[test]
    fn a_rich_draft_comes_back_with_its_format_and_signature() {
        let acc = crate::account::Account::new("mail.example.com".into(), "me@example.com".into());
        let draft = crate::smtp::Draft {
            to: "you@example.com".into(),
            subject: "Hi".into(),
            body: "Hello\n\n-- \nJane".into(),
            html: Some(
                "<p><b>Hello</b></p><div data-signature>-- <br><b style=\"color:#1f6feb\">Jane</b></div>".into(),
            ),
            message_id: Some("<d1@example.com>".into()),
            cc: String::new(),
            bcc: String::new(),
            in_reply_to: None,
            references: Vec::new(),
            markdown: false,
            attachments: Vec::new(),
            from: None,
        };
        let raw = crate::smtp::build_draft(&acc, &draft, None).unwrap().formatted();
        let view = parse_view(&raw);
        assert_eq!(view.draft_format.as_deref(), Some("rich"));
        let html = view.html.unwrap();
        assert!(
            html.contains("<b>Hello</b>") && html.contains("data-signature") && html.contains("color:#1f6feb"),
            "{html}"
        );
        let plain = crate::smtp::build_draft(&acc, &crate::smtp::Draft { html: None, ..draft }, None)
            .unwrap()
            .formatted();
        assert_eq!(parse_view(&plain).draft_format.as_deref(), Some("plain"));
    }

    use super::*;

    const SAMPLE: &[u8] = b"From: Anna <anna@example.com>\r\n\
To: me@example.com\r\n\
Subject: Report\r\n\
Content-Type: multipart/mixed; boundary=\"outer\"\r\n\
\r\n\
--outer\r\n\
Content-Type: multipart/related; boundary=\"inner\"\r\n\
\r\n\
--inner\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Logo: <img src=\"cid:logo@x\"> remote: <img src=\"https://tracker.example/p.gif\"></p><script>alert(1)</script>\r\n\
--inner\r\n\
Content-Type: image/png\r\n\
Content-ID: <logo@x>\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw0KGgo=\r\n\
--inner--\r\n\
--outer\r\n\
Content-Type: application/pdf; name=\"report.pdf\"\r\n\
Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQ=\r\n\
--outer--\r\n";

    #[test]
    fn inline_images_attachments_and_remote_content() {
        let v = parse_view(SAMPLE);
        let html = v.html.unwrap();
        assert!(html.contains("data:image/png;base64,iVBORw0KGgo="), "{html}");
        assert!(html.contains("https://tracker.example/p.gif"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(v.has_remote_content);
        assert_eq!(v.attachments.len(), 1, "{:?}", v.attachments);
        assert_eq!(v.attachments[0].name, "report.pdf");
        assert_eq!(v.attachments[0].mime, "application/pdf");
        let (name, bytes) = attachment(SAMPLE, v.attachments[0].index).unwrap();
        assert_eq!(name, "report.pdf");
        assert_eq!(bytes, b"%PDF-1.4");
        assert!(parse_card(1, SAMPLE, false, false, 0).has_attachments);
    }

    #[test]
    fn card_keeps_to_apart_from_cc() {
        let raw = b"From: a@example.com\r\nTo: Me <me@example.com>, b@example.com\r\nCc: c@example.com\r\nSubject: x\r\n\r\nhi";
        let card = parse_card(1, raw, false, false, 0);
        assert_eq!(card.to, "me@example.com, b@example.com, c@example.com");
        assert_eq!(card.to_only, "me@example.com, b@example.com");
    }

    #[test]
    fn parses_calendar_invitations() {
        let ics = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Rome\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:abc-123@example.com\r\nSUMMARY:Project kick-off\\, phase 2\r\nLOCATION:Room 4\r\nDTSTART;TZID=Europe/Rome:20261028T100000\r\nDTEND;TZID=Europe/Rome:20261028T113000\r\nORGANIZER;CN=\"Anna: PM\":mailto:anna@example.com\r\nATTENDEE;CN=Me:mailto:me@example.com\r\nDESCRIPTION:A long description that continues\r\n  on the next line\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let inv = parse_ics(ics).unwrap();
        assert_eq!(inv.method, "REQUEST");
        assert_eq!(inv.uid, "abc-123@example.com");
        assert_eq!(inv.summary, "Project kick-off, phase 2");
        assert_eq!(inv.location, "Room 4");
        assert_eq!(
            inv.start,
            IcsTime {
                date: "2026-10-28".into(),
                time: Some("10:00".into()),
                utc: false
            }
        );
        assert_eq!(inv.end.unwrap().time.as_deref(), Some("11:30"));
        assert_eq!(inv.organizer, "anna@example.com");
        assert_eq!(inv.organizer_name, "Anna: PM");
        let all_day = parse_ics("BEGIN:VEVENT\nUID:x\nDTSTART;VALUE=DATE:20261101\nEND:VEVENT\n").unwrap();
        assert_eq!(all_day.start.time, None);
        let utc = parse_ics("BEGIN:VEVENT\nUID:y\nDTSTART:20261028T090000Z\nEND:VEVENT\n").unwrap();
        assert!(utc.start.utc);
    }

    #[test]
    fn recognises_automated_senders() {
        let card = |raw: &str| parse_card(1, raw.as_bytes(), false, false, 0);
        let person = "From: Anna <anna@example.com>\r\nSubject: Hi\r\n\r\nHello";
        assert!(!card(person).automated);
        assert!(card("From: GitHub <notifications@github.com>\r\nSubject: PR\r\n\r\nx").automated);
        assert!(card("From: Shop <info@shop.example>\r\nList-Unsubscribe: <mailto:u@shop.example>\r\n\r\nx").automated);
        assert!(card("From: Bot <bot@example.com>\r\nAuto-Submitted: auto-generated\r\n\r\nx").automated);
        assert!(card("From: Bank <no-reply@bank.example>\r\n\r\nx").automated);
        assert!(!card("From: Bob <bob@example.com>\r\nAuto-Submitted: no\r\n\r\nx").automated);
    }

    #[test]
    fn finds_quoted_text() {
        let gmail = "<div>Thanks!</div><br><div class=\"gmail_quote\"><div>On Mon, Bob wrote:</div><blockquote>old</blockquote></div>";
        assert_eq!(&gmail[..quote_start_html(gmail).unwrap()], "<div>Thanks!</div><br>");
        let apple = "<div>Ok</div><div><br><div>On 27 Oct 2026, at 10:00, Ann wrote:</div><br><blockquote type=\"cite\">old</blockquote></div>";
        assert!(apple[..quote_start_html(apple).unwrap()].ends_with("<div>Ok</div><div><br>"));
        assert_eq!(quote_start_html("<p>no quote here</p>"), None);

        let text = "Sounds good.\n\nOn Mon, 26 Oct 2026, Bob wrote:\n> hi\n> there\n";
        assert_eq!(&text[..quote_start_text(text).unwrap()], "Sounds good.\n\n");
        let it = "Va bene.\nIl giorno lun 26 ott 2026, Bob ha scritto:\n> ciao\n";
        assert_eq!(&it[..quote_start_text(it).unwrap()], "Va bene.\n");
        let outlook = "Ciao\n\nDa: Bob\nInviato: lunedì\nA: me\n\nold";
        assert_eq!(&outlook[..quote_start_text(outlook).unwrap()], "Ciao\n\n");
        assert_eq!(quote_start_text("> only a quote\n"), None);
        assert_eq!(quote_start_text("No quote.\nJust text.\n"), None);
    }

    const WITH_FILES: &str = "From: Anna Bianchi <anna@example.com>\r\n\
To: me@example.com\r\n\
Subject: Slides\r\n\
Message-ID: <slides@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Here are the slides.\r\n\
--b1\r\n\
Content-Type: application/pdf; name=\"slides.pdf\"\r\n\
Content-Disposition: attachment; filename=\"slides.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4Kc3RyZWFtCnRoaXMgaXMgYSBmYWtlIHBkZgplbmRzdHJlYW0K\r\n\
JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSPj4Kc3RyZWFtCnRoaXMgaXMgYSBmYWtlIHBkZgplbmRzdHJlYW0K\r\n\
--b1\r\n\
Content-Type: image/png; name=\"photo.png\"\r\n\
Content-Disposition: attachment; filename=\"photo.png\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw0KGgo=\r\n\
--b1--\r\n";

    #[test]
    fn removing_an_attachment_leaves_a_note_and_the_rest_unchanged() {
        let raw = WITH_FILES.as_bytes();
        let (out, saved) = strip_attachments(raw, &[0], "27 Nov 2026").unwrap();
        assert!(saved > 100, "{saved}");
        let text = String::from_utf8(out.clone()).unwrap();
        // Headers and text byte for byte, the other attachment too.
        let head = WITH_FILES
            .split("--b1\r\nContent-Type: application/pdf")
            .next()
            .unwrap();
        let (mark, rest) = text.split_once("\r\n").unwrap();
        assert!(mark.starts_with("X-Pigeonhole-Removed: "), "{mark}");
        assert!(rest.starts_with(head), "the mark first, then the headers as they were");
        // Its card knows it is the lighter copy; the photo left keeps the paperclip.
        let card = parse_card(1, &out, false, false, 0);
        assert_eq!(
            card.removed_mark.as_deref(),
            Some(&mark["X-Pigeonhole-Removed: ".len()..])
        );
        assert!(card.has_attachments);
        assert!(text.ends_with("--b1\r\nContent-Type: image/png; name=\"photo.png\"\r\nContent-Disposition: attachment; filename=\"photo.png\"\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--b1--\r\n"));
        // It reads as before, with the note in place of the PDF.
        let view = parse_view(&out);
        assert_eq!(view.message_id.as_deref(), Some("slides@example.com"));
        let names: Vec<_> = view.attachments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["photo.png"]);
        assert_eq!(view.text.as_deref(), Some("Here are the slides."));
        assert_eq!(
            view.removed,
            [RemovedAttachment {
                what: "slides.pdf, 162 B".into(),
                day: "27 Nov 2026".into()
            }]
        );
        // Both at once.
        let (both, _) = strip_attachments(raw, &[1, 0], "27 Nov 2026").unwrap();
        assert!(parse_view(&both).attachments.is_empty());
        // Nothing left: no paperclip, though the email is still "mixed".
        assert!(!parse_card(1, &both, false, false, 0).has_attachments);
        // Each removal has its own mark.
        assert_ne!(parse_card(1, &both, false, false, 0).removed_mark, card.removed_mark);
    }

    #[test]
    fn signed_emails_and_whole_email_attachments_are_left_alone() {
        let signed = WITH_FILES.replace(
            "multipart/mixed; boundary=\"b1\"",
            "multipart/signed; protocol=\"application/pkcs7-signature\"; boundary=\"b1\"",
        );
        assert!(strip_attachments(signed.as_bytes(), &[0], "x")
            .unwrap_err()
            .contains("signed"));
        let alone = "From: a@example.com\r\nSubject: s\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\nContent-Disposition: attachment; filename=\"a.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n";
        assert!(strip_attachments(alone.as_bytes(), &[0], "x").is_err());
        assert!(
            strip_attachments(WITH_FILES.as_bytes(), &[5], "x").is_err(),
            "an index that is not there"
        );
    }
}
