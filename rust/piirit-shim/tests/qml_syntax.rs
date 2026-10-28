//! QML rules host Qt can't check: syntax newer than Sailfish Qt 5.6, row heights.
//! Text scan: host Qt 5.15 accepts newer syntax, only warns. Device handlers never fire.

use std::fs;
use std::path::{Path, PathBuf};

/// Element declaring `marker`, to next blank line.
/// Whole-file `contains` can't tell which element a string is from: once two carry it, the
/// check can no longer fail.
fn block_of(text: &str, marker: &str) -> String {
    text.lines()
        .skip_while(|line| !line.contains(marker))
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn qml_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "qml") {
                out.push(path);
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    files
}

#[test]
fn qml_avoids_qt_5_15_only_signal_handler_syntax() {
    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read qml");
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("function on") {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "`function onFoo() {{ ... }}` inside Connections is Qt 5.15+ syntax. \
         Sailfish runs Qt 5.6, where it is not an error but is never connected, \
         so the handler silently never runs. Use `onFoo: {{ ... }}` with the \
         shim's snake_case parameter names instead.\n  {}",
        offenders.join("\n  ")
    );
}

/// Platform enum property bound by name, never number.
///
/// Qt's QML compiler checks a literal on an enum property against key names; "2" is none,
/// so the file fails to compile. For a window-created component that is the whole window:
/// white screen (as with `urgency: 2` on a Nemo notification). Host runs miss it: QML stubs
/// can declare an enum but not a property of one, so theirs are `int`s.
///
/// Properties below are enum-typed on device; a name is listed only once the tree binds it.
#[test]
fn platform_enum_properties_are_bound_by_name() {
    const ENUM_PROPERTIES: &[(&str, &str)] = &[
        (
            "urgency",
            "Nemo.Notifications' Notification.urgency (enum Urgency)",
        ),
        ("bus", "Nemo.DBus' DBusInterface.bus (enum BusType)"),
        (
            "truncationMode",
            "Silica's truncationMode (enum TruncationMode.Mode)",
        ),
        (
            "horizontalAlignment",
            "QtQuick's horizontalAlignment (enum HAlignment)",
        ),
        ("wrapMode", "QtQuick's wrapMode (enum WrapMode)"),
        ("textFormat", "QtQuick's textFormat (enum TextFormat)"),
    ];
    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read qml");
        for (number, line) in code_only(&text).lines().enumerate() {
            let Some((name, value)) = line.trim().split_once(':') else {
                continue;
            };
            let value = value.trim().trim_end_matches(';').trim();
            let numeric = !value.is_empty()
                && value
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit());
            if let Some((_, what)) = ENUM_PROPERTIES.iter().find(|(key, _)| *key == name.trim()) {
                if numeric {
                    offenders.push(format!(
                        "{}:{}: {} -- {what}",
                        file.display(),
                        number + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "an enum property bound to a number: Qt 5.6 refuses the file \
         (\"Invalid property assignment: unknown enumeration\"), and the \
         host stubs cannot tell. Use the enum's name.\n  {}",
        offenders.join("\n  ")
    );
}

/// Wrapping `Label` row takes height from it; constant height overlaps neighbours.
///
/// Scan, not measurement: `tests/qml_conversation_list.rs` loads the list, but a collapsed
/// row still passes its scrolling assertions.
#[test]
fn wrapping_list_rows_size_to_their_text() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components/ConversationList.qml");
    let text = fs::read_to_string(&path).expect("read ConversationList.qml");

    // Whole binding: it wraps. First line alone misjudges height.
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim_start().starts_with("contentHeight:"))
        .expect("ConversationList.qml states a row's contentHeight");
    let mut height = lines[start].trim().to_string();
    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if !trimmed.starts_with(['+', '-', '*', '/', '?', ':', '&', '|']) {
            break;
        }
        height.push(' ');
        height.push_str(trimmed);
    }
    assert!(
        height.contains("body.height"),
        "the message row's contentHeight does not follow the delegate it \
         holds; what the delegate itself measures is in \
         tests/qml_conversation.rs: {height:?}"
    );
}

/// Delegate's `model.<role>` must exist on model: rename = blank row, uncaught.
#[test]
// Long for its table: one row per page with a model.
#[allow(clippy::too_many_lines)]
fn delegates_bind_only_roles_their_models_have() {
    /// Roles a file reads.
    fn bound_roles(text: &str) -> Vec<String> {
        text.split("model.")
            .skip(1)
            .map(|tail| {
                tail.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .filter(|role| !role.is_empty())
            .collect()
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases: [(&str, Vec<String>); 16] = [
        (
            "qml/components/ConversationList.qml",
            names_of::<piirit_shim::MessageListItem>(),
        ),
        // Media pages: message rows.
        (
            "qml/pages/ChatMediaPage.qml",
            names_of::<piirit_shim::MessageListItem>(),
        ),
        (
            "qml/pages/GalleryPage.qml",
            names_of::<piirit_shim::MessageListItem>(),
        ),
        (
            "qml/pages/ChatListPage.qml",
            names_of::<piirit_shim::ChatListItem>(),
        ),
        // One chat list per profile: people as JSON, `modelData`.
        (
            "qml/cover/CoverPage.qml",
            names_of::<piirit_shim::AccountItem>(),
        ),
        // Chats and profiles.
        (
            "qml/pages/ChatPickerPage.qml",
            [
                names_of::<piirit_shim::ChatListItem>(),
                names_of::<piirit_shim::AccountItem>(),
            ]
            .concat(),
        ),
        (
            "qml/pages/ProfilesPage.qml",
            names_of::<piirit_shim::AccountItem>(),
        ),
        (
            "qml/pages/NewChatPage.qml",
            names_of::<piirit_shim::ContactItem>(),
        ),
        (
            "qml/pages/GroupPage.qml",
            names_of::<piirit_shim::ContactItem>(),
        ),
        (
            "qml/pages/AddMembersPage.qml",
            names_of::<piirit_shim::ContactItem>(),
        ),
        (
            "qml/pages/CommonChatsPage.qml",
            names_of::<piirit_shim::ChatListItem>(),
        ),
        // The contact, chats in common.
        (
            "qml/pages/ContactPage.qml",
            [
                names_of::<piirit_shim::ContactItem>(),
                names_of::<piirit_shim::ChatListItem>(),
            ]
            .concat(),
        ),
        // Block list, picker: contacts.
        (
            "qml/pages/BlockedContactsPage.qml",
            names_of::<piirit_shim::ContactItem>(),
        ),
        (
            "qml/pages/BlockContactPage.qml",
            names_of::<piirit_shim::ContactItem>(),
        ),
        (
            "qml/components/SearchResultsList.qml",
            names_of::<piirit_shim::SearchItem>(),
        ),
        // Profile relay rows: one transport each.
        (
            "qml/pages/ProfilePage.qml",
            names_of::<piirit_shim::TransportItem>(),
        ),
    ];

    for (file, roles) in cases {
        let text =
            fs::read_to_string(root.join(file)).unwrap_or_else(|err| panic!("read {file}: {err}"));
        let bound = bound_roles(&text);
        assert!(
            !bound.is_empty(),
            "{file} binds nothing from its model any more"
        );
        for role in &bound {
            assert!(
                roles.contains(role),
                "{file} binds model.{role}, which its model does not have: {roles:?}"
            );
        }
    }
}

/// New-group rows: `modelData.<name>` from `ContactList.picked_rows`; names must still be
/// a contact's. Page binds no roles, so is not in the case list above; without this it would
/// bind anything.
#[test]
fn the_new_group_page_reads_only_what_a_contact_row_carries() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/NewGroupPage.qml");
    let text = fs::read_to_string(&path).expect("read NewGroupPage.qml");
    let names = names_of::<piirit_shim::ContactItem>();
    let bound: Vec<String> = text
        .split("modelData.")
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    assert!(
        !bound.is_empty(),
        "the new group page draws no member rows any more"
    );
    for name in &bound {
        assert!(
            names.contains(name),
            "NewGroupPage.qml reads modelData.{name}, which a contact does \
             not carry: {names:?}"
        );
    }
}

/// New chat contact list must clip: anchored under search field, rows draw over it else.
#[test]
fn the_new_chat_list_clips_away_from_its_search_field() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/NewChatPage.qml");
    let text = fs::read_to_string(&path).expect("read NewChatPage.qml");
    assert!(
        block_of(&text, "id: listView").contains("clip: true"),
        "NewChatPage.qml's contact list does not clip, so its rows run \
         over the search field above it"
    );
}

/// Every row on a page with search field marks search (Marks.js): one list without
/// marks reads as different app. Rows: `match:`, set or not, whatever they list.
#[test]
fn every_searchable_list_marks_what_was_found() {
    const ROWS: [&str; 2] = ["ContactRow {", "ChatListDelegate {"];
    let mut searchable = 0;
    let mut offenders = Vec::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        if !text.contains("SearchField {") {
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !ROWS.contains(&trimmed) {
                continue;
            }
            searchable += 1;
            // Element body: to `}` at element's own indent.
            let indent = line.len() - line.trim_start().len();
            let marks = lines[number + 1..]
                .iter()
                .take_while(|next| {
                    !(next.trim() == "}" && next.len() - next.trim_start().len() == indent)
                })
                .any(|next| next.trim_start().starts_with("match:"));
            if !marks {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }
    assert!(
        searchable >= 5,
        "found only {searchable} rows on searchable pages; scan lost them"
    );
    assert!(
        offenders.is_empty(),
        "these rows sit under a search field but do not mark what it found \
         (`match:`):\n  {}",
        offenders.join("\n  ")
    );
}

/// Time label positions off row, not off label its width decides.
/// `SearchResultRow` had a cycle (title width minus time label's; time label anchored to
/// title): on device title kept natural width, long names ran off edge.
/// `ChatListDelegate` is the working shape (time label off `root.width`, name width off
/// its `x`). Offscreen engine hides it: scan, not measure.
#[test]
fn a_time_label_does_not_depend_on_the_label_it_sizes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    for (file, time_id, sized_id) in [
        ("SearchResultRow.qml", "id: timeLabel", "titleLabel"),
        ("ChatListDelegate.qml", "id: timeLabelItem", "nameLabel"),
    ] {
        let text =
            fs::read_to_string(root.join(file)).unwrap_or_else(|err| panic!("read {file}: {err}"));
        let block = block_of(&text, time_id);
        assert!(
            !block.is_empty(),
            "{file} has no {time_id} any more; this check is measuring nothing"
        );
        assert!(
            !block.contains(sized_id),
            "{file}'s time label positions itself off {sized_id}, whose width \
             is decided by this label -- that is the loop that put a long \
             title off the side of the screen. Place it off the row instead, \
             as ChatListDelegate does. Block was:\n{block}"
        );
    }
}

/// Roles a model row exposes to QML.
fn names_of<T: qmetaobject::listmodel::SimpleListItem>() -> Vec<String> {
    T::names().iter().map(ToString::to_string).collect()
}

/// List draws delegates outside its box unless clipped; both list pages sit above translucent
/// things (message field, error banner) content would show through.
#[test]
fn list_pages_clip_and_leave_room_for_what_sits_below_them() {
    for (page, list) in [
        ("ConversationPage.qml", "../components/ConversationList.qml"),
        ("ChatListPage.qml", "ChatListPage.qml"),
    ] {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages");
        let text =
            fs::read_to_string(dir.join(page)).unwrap_or_else(|err| panic!("read {page}: {err}"));
        let list_text =
            fs::read_to_string(dir.join(list)).unwrap_or_else(|err| panic!("read {list}: {err}"));
        assert!(
            list_text.contains("clip: true"),
            "{list} does not clip, so its rows draw over what is below them"
        );
        // Scoped to list block: jump button anchors to same thing, so whole-file search matches
        // whatever list does.
        let list_block = block_of(&text, "id: listView");
        assert!(
            list_block.contains("bottom: banner.top"),
            "{page}'s list runs under the banner instead of stopping at it"
        );
        // Losing core is a state, not an event: page opened later missed the transition, so
        // banner reads status. Both states: a page knowing only "stopped" says "restart
        // Piirit" during a reconnect already under way.
        for state in ["reconnecting", "stopped"] {
            assert!(
                text.contains(&format!("core.status === \"{state}\"")),
                "{page} does not read the core's {state} state, so it waits \
                 for a transition it may never see"
            );
        }
        assert!(
            text.contains("page.coreStatusMessage.length > 0"),
            "{page}'s banner does not prefer the core's own state over the \
             last error, so a dead core hides behind whatever failed first"
        );
    }
}

/// Placeholder says "empty" only after model answered.
///
/// Empty model means nothing there or no answer yet; count alone flashed "No contacts" while
/// loading. Every count-reading placeholder also reads whether count is an answer.
/// Profiles page exempt: its list is core's account list, read before chat list is shown.
/// One line at a time: wrapped binding escapes.
#[test]
fn no_list_says_it_is_empty_before_it_has_been_answered() {
    let mut checked = 0;
    let mut offenders = Vec::new();
    for file in qml_files() {
        if file.ends_with("ProfilesPage.qml") {
            continue;
        }
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        let lines: Vec<&str> = code.lines().collect();
        for (start, line) in lines.iter().enumerate() {
            if !line.contains("ViewPlaceholder {") {
                continue;
            }
            // Placeholder's own `enabled`, not nested one.
            let (mut opened, mut closed) = (0, 0);
            for (number, inner) in lines.iter().enumerate().skip(start) {
                let binding = inner.trim();
                if opened == closed + 1
                    && binding.starts_with("enabled:")
                    && binding.contains("count")
                {
                    checked += 1;
                    if !binding.to_lowercase().contains("loaded") {
                        offenders.push(format!("{}:{}: {binding}", file.display(), number + 1));
                    }
                }
                opened += inner.matches('{').count();
                closed += inner.matches('}').count();
                if closed >= opened {
                    break;
                }
            }
        }
    }
    // Else passes by finding nothing.
    assert!(
        checked >= 10,
        "only {checked} placeholders read a count; there should be ten -- \
         the chat list and its search, the forwarding picker, the \
         conversation, a chat's media twice, and four contact lists"
    );
    assert!(
        offenders.is_empty(),
        "these say a list is empty before the core has answered, so the \
         placeholder flashes over rows about to arrive; read a flag set \
         once the model has answered, as NewChatPage's `contactsLoaded` \
         is:\n  {}",
        offenders.join("\n  ")
    );
}

/// Copy says so. Reply bar and jump button are the tested components, not one-off items.
#[test]
fn the_conversation_page_uses_the_pieces_that_are_tested() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/ConversationPage.qml");
    let text = fs::read_to_string(&path).expect("read ConversationPage.qml");

    for piece in ["ReplyBar {", "JumpButton {", "Banner {"] {
        assert!(
            text.contains(piece),
            "the page does not use {piece} -- what a test measures is then not what runs"
        );
    }
    assert!(
        text.contains("Clipboard.text = body") && text.contains("notice.show("),
        "copying a message does not say that it happened"
    );
    // Right inset must match left, or send sits nearer edge. Scoped to row: jump button has
    // same inset, so whole-file search can't fail.
    assert!(
        block_of(&text, "id: inputRow").contains("rightMargin: Theme.horizontalPageMargin"),
        "the send button is flush against the edge of the screen"
    );
    // Flush draft and pending deletes on leave (timers not fired).
    // `qml_delete_run.rs` covers list end; loading list alone can't prove page asks.
    assert!(
        block_of(&text, "PageStatus.Deactivating").contains("listView.flushDeletes()"),
        "leaving the chat does not send the deletes still waiting, so a \
         reader who asked for a message to go and then left keeps it"
    );
    // Both delete ends, neither loadable headless: menu leads to page asking which kind;
    // its answer picks which of core's two calls. One call for both would delete from
    // everyone's phone what reader meant for own, or the reverse. Irreversible.
    let choosing = block_of(&text, "onDeleteChoiceRequested");
    assert!(
        choosing.contains("DeleteMessageDialog.qml") && choosing.contains("confirmDelete"),
        "the conversation does not ask which kind of delete a message is, \
         or never tells the list what was answered"
    );
    let deleting = block_of(&text, "onDeleteRequested");
    assert!(
        deleting.contains("messages.delete_message_for_all(messageId)")
            && deleting.contains("messages.delete_message(messageId)"),
        "the conversation does not make both of the core's deletions, so \
         one of the two answers the reader can give is not the one acted on"
    );
    // Return = line break only, as in reference clients; sending is the button's. Tests strip
    // `EnterKey.` lines (common::qml_tree_without_enter_key), so shipped file must carry none.
    assert!(
        !text.contains("EnterKey."),
        "the message field gives the return key a job of its own, when it \
         is only ever a line break"
    );
}

/// Remote text must say plain: default `Text.AutoText` sniffs markup, so a body of
/// `<img src="https://tracker/p.gif">` fetches it on draw (network must not watch). Also
/// mismeasures: hidden bubble-sizing copies measure rendered width.
///
/// `PageHeader`, `SectionHeader`, `ViewPlaceholder` have no `textFormat`: remote string in
/// one is an offender outright (conversation header once carried chat name so).
/// `ConversationHeader` replaces it.
#[test]
fn text_from_the_other_end_is_pinned_to_plain() {
    // Bindings core fills from message, contact or chat: remote input.
    const REMOTE: [&str; 31] = [
        "model.",
        "root.messageText",
        // Composed or search-marked name, preview (Marks.js).
        "root.nameText",
        "root.marked",
        "root.quoteText",
        "root.quoteAuthor",
        "root.senderName",
        "root.chatName",
        "root.preview",
        "root.previewSender",
        "root.fileName",
        "root.filePath",
        "root.author",
        "root.body",
        "root.text",
        "root.title",
        "root.subtitle",
        "root.displayName",
        "root.address",
        "root.initial",
        "root.vcardName",
        "root.vcardAddr",
        "root.webxdcName",
        "root.webxdcDocument",
        "root.webxdcSummary",
        "root.genericText",
        "page.chatName",
        "page.fileName",
        "page.fileMime",
        "page.senderName",
        "page.myInvite",
    ];

    /// Element a line sits in: nearest `Foo {` above.
    fn element_of(lines: &[&str], index: usize) -> String {
        lines[..index]
            .iter()
            .rev()
            .find_map(|line| {
                let trimmed = line.trim();
                let name = trimmed.strip_suffix('{')?.trim();
                (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '.'))
                    .then(|| name.to_string())
            })
            .unwrap_or_default()
    }

    let mut offenders = Vec::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let is_text = trimmed.starts_with("text:");
            let is_title = trimmed.starts_with("title:");
            if !is_text && !is_title {
                continue;
            }
            // Only own-text elements. MenuItem label is translated literal.
            let element = element_of(&lines, number);
            let renders = (element == "Label" || element == "Text") && is_text;
            let unpinnable = matches!(
                element.as_str(),
                "PageHeader" | "SectionHeader" | "ViewPlaceholder"
            );
            if !renders && !unpinnable {
                continue;
            }
            // Binding continues while lines indented past `text:`.
            let indent = line.len() - trimmed.len();
            let mut value = trimmed.to_string();
            for next in &lines[number + 1..] {
                let next_indent = next.len() - next.trim_start().len();
                if next.trim().is_empty() || next_indent <= indent {
                    break;
                }
                value.push(' ');
                value.push_str(next.trim());
            }
            if !REMOTE.iter().any(|name| value.contains(name)) {
                continue;
            }
            if unpinnable {
                offenders.push(format!(
                    "{}:{}: {trimmed} ({element} cannot be told this is plain text; \
                     draw it in a Label of your own, as ConversationHeader does)",
                    file.display(),
                    number + 1
                ));
                continue;
            }
            // `textFormat` sits in same element block, from `{` above.
            let start = number.saturating_sub(16);
            let block = lines[start..=number].join("\n");
            if !block.contains("textFormat:") {
                offenders.push(format!("{}:{}: {trimmed}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these show text the other end chose without saying it is plain, so \
         Qt decides for itself whether it is markup:\n  {}",
        offenders.join("\n  ")
    );
}

/// Binding from `lines[at]` on, continuation lines (indented past it) joined.
fn binding(lines: &[&str], at: usize) -> String {
    let indent = lines[at].len() - lines[at].trim_start().len();
    let mut value = lines[at].trim().to_string();
    for next in &lines[at + 1..] {
        if next.trim().is_empty() || next.len() - next.trim_start().len() <= indent {
            break;
        }
        value.push(' ');
        value.push_str(next.trim());
    }
    value
}

/// Identifiers and words of `text`.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
}

/// `value` without what it only compares (`a === name ? …`: name not drawn).
fn uncompared(value: &str) -> String {
    const COMPARES: [&str; 4] = ["===", "!==", "==", "!="];
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let compares = |at: Option<usize>| {
        at.and_then(|at| tokens.get(at))
            .is_some_and(|token| COMPARES.contains(token))
    };
    tokens
        .iter()
        .enumerate()
        .filter(|(at, _)| !compares(Some(at + 1)) && !compares(at.checked_sub(1)))
        .map(|(_, token)| *token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every label drawing someone's name draws its emoji as Twemoji (Emoji.js `inText` or
/// `drawn`): Sailfish's font has few, so a name shows boxes on one page, pictures on next.
/// Label's text binding must name a property its file builds from `Emoji.`.
#[test]
fn every_name_draws_its_emoji_as_pictures() {
    // Bindings naming a peer, chat or contact.
    const NAMES: [&str; 8] = [
        "peer_name",
        "senderName",
        "quoteAuthor",
        "chatName",
        "displayName",
        "contactName",
        "vcardName",
        "nameText",
    ];
    let mut names = 0;
    let mut offenders = Vec::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        let lines: Vec<&str> = text.lines().collect();
        // `readonly property string x: …` definitions, name and value.
        let properties: Vec<(&str, String)> = (0..lines.len())
            .filter_map(|at| {
                let rest = lines[at].trim().strip_prefix("readonly property string ")?;
                let (name, _) = rest.split_once(':')?;
                Some((name.trim(), binding(&lines, at)))
            })
            .collect();
        // Built from Emoji.js, or from one that is; names held under another name (e.g.
        // `plain`) too.
        let mut pictured: Vec<&str> = Vec::new();
        loop {
            let more: Vec<&str> = properties
                .iter()
                .filter(|(name, value)| {
                    !pictured.contains(name)
                        && (value.contains("Emoji.")
                            || words(value).any(|word| pictured.contains(&word)))
                })
                .map(|(name, _)| *name)
                .collect();
            if more.is_empty() {
                break;
            }
            pictured.extend(more);
        }
        let mut held: Vec<&str> = NAMES.to_vec();
        held.extend(
            properties
                .iter()
                .filter(|(name, value)| {
                    !pictured.contains(name) && words(value).any(|word| NAMES.contains(&word))
                })
                .map(|(name, _)| *name),
        );
        for (number, line) in lines.iter().enumerate() {
            if !line.trim_start().starts_with("text:") {
                continue;
            }
            let value = binding(&lines, number);
            let drawn = uncompared(&value);
            if !words(&drawn).any(|word| held.contains(&word)) {
                continue;
            }
            // Own-text elements only; nearest `Foo {` above (capitalised: `anchors {` is a group).
            let element = lines[..number]
                .iter()
                .rev()
                .filter_map(|above| above.trim().strip_suffix('{').map(str::trim))
                .find(|name| name.starts_with(char::is_uppercase))
                .unwrap_or_default();
            if element != "Label" && element != "Text" {
                continue;
            }
            names += 1;
            if !words(&value).any(|word| pictured.contains(&word)) {
                offenders.push(format!("{}:{}: {value}", file.display(), number + 1));
            }
        }
    }
    assert!(names >= 8, "found only {names} name labels; scan lost them");
    assert!(
        offenders.is_empty(),
        "these draw a name without its emoji as pictures (Emoji.js `inText`):\n  {}",
        offenders.join("\n  ")
    );
}

/// dconf keys named only in `Settings.qml`; all app's own but platform's few it reads.
/// Second naming file = second definition, drift.
#[test]
fn only_the_settings_object_names_the_dconf_keys() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let mut elsewhere = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        let named: Vec<String> = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("key: \""))
            .filter_map(|rest| rest.split('"').next())
            .map(ToString::to_string)
            .collect();
        if named.is_empty() {
            continue;
        }
        if file == root.join("components/Settings.qml") {
            keys.extend(named);
        } else {
            elsewhere.push(file.display().to_string());
        }
    }
    assert!(
        elsewhere.is_empty(),
        "these name a dconf key outside components/Settings.qml, which is \
         the one place the keys are defined:\n  {}",
        elsewhere.join("\n  ")
    );
    assert!(
        keys.len() >= 5,
        "Settings.qml names fewer keys than the settings it exists for -- \
         Markdown, clean links, the download limit, notification detail, \
         and whether webxdc apps are offered: {keys:?}"
    );
    let foreign: Vec<&String> = keys
        .iter()
        .filter(|key| !key.starts_with("/apps/harbour-piirit/"))
        .filter(|key| !PLATFORM_KEYS_READ.contains(&key.as_str()))
        .collect();
    assert!(
        foreign.is_empty(),
        "a key is outside the app's own dconf path, and not one of the platform's \
         it reads: {foreign:?}"
    );

    // Platform keys: read, never written (read-only properties, nothing sets object).
    let settings = fs::read_to_string(root.join("components/Settings.qml")).expect("read");
    for key in PLATFORM_KEYS_READ {
        assert!(
            settings.contains(&format!("key: \"{key}\"")),
            "{key} is listed as read but Settings.qml no longer names it"
        );
    }
    assert!(
        settings.contains("readonly property var backLensLabels: backLensLabelsValue.value"),
        "the platform's lens labels are not a read-only property"
    );
    let writers: Vec<String> = qml_files()
        .into_iter()
        .filter(|file| {
            let text = fs::read_to_string(file).expect("read qml");
            text.contains("backLensLabelsValue.value =")
                || text.contains("backLensLabelsConfig.value =")
                || text.contains("alias backLensLabels")
        })
        .map(|file| file.display().to_string())
        .collect();
    assert!(
        writers.is_empty(),
        "these could write the platform camera's own key: {writers:?}"
    );
}

/// Platform keys outside app path: read only; lens labels for camera.
const PLATFORM_KEYS_READ: [&str; 1] = ["/apps/jolla-camera/backCameraLabels"];

/// File reading its directory's singleton imports it: `import "."`.
///
/// Qt 5.6 reads `qmldir` only for an import naming the dir; implicit own-dir import skips
/// it. Without the line, `Settings` beside `Settings.qml` is the plain type: reads throw,
/// bindings keep defaults, writes miss dconf. Host Qt 5.15 gives singleton anyway: quick
/// actions settings passed all tests and did nothing on phone.
#[test]
fn a_singleton_read_beside_it_is_imported_by_its_directory() {
    let mut offenders = Vec::new();
    let mut checked = 0;
    for file in qml_files() {
        let dir = file.parent().expect("a file is in a directory");
        let Ok(qmldir) = fs::read_to_string(dir.join("qmldir")) else {
            continue;
        };
        let text = fs::read_to_string(&file).expect("read qml");
        let imports_itself = text.lines().any(|line| line.trim() == "import \".\"");
        let used = tokens(&code_only(&text));
        for line in qmldir.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let ["singleton", name, _, source] = words.as_slice() else {
                continue;
            };
            if dir.join(source) == file {
                continue;
            }
            checked += 1;
            if used.iter().any(|token| token == name) && !imports_itself {
                offenders.push(format!("{} reads {name}", file.display()));
            }
        }
    }
    assert!(
        checked > 0,
        "found no file beside a singleton to check; has the qmldir moved?"
    );
    assert!(
        offenders.is_empty(),
        "these read a singleton declared in their own directory's qmldir \
         without `import \".\"`, which on Qt 5.6 leaves the name the plain \
         type:\n  {}",
        offenders.join("\n  ")
    );
}

/// No deletion wait on the row it was asked on.
///
/// `ListItem.remorseAction` puts wait in row; deleting from a list destroys rows, so a run
/// of deletes lost most of itself. Action lives in list (`components/PendingRemoval.qml`);
/// look is still Silica's `RemorseItem`, raised by row with no-op callback
/// (`every_pending_removal_is_drawn_by_the_platform`). Bans shortcut, not widget.
#[test]
fn no_wait_before_a_deletion_lives_on_the_row_it_was_asked_on() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        for (number, line) in code.lines().enumerate() {
            if line.contains("remorseAction(") {
                offenders.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these put the wait before a deletion on the row it was asked \
         for on, which the deletion itself destroys; use a \
         PendingRemoval beside the list instead:\n  {}",
        offenders.join("\n  ")
    );
}

/// Wait drawn by platform: Silica's `RemorseItem` (bar, seconds, "Tap to cancel").
/// Hand-made stand-in looks wrong on phone.
#[test]
fn every_pending_removal_is_drawn_by_the_platform() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        if !code.contains("PendingRemoval {") {
            continue;
        }
        if !code.contains("RemorseItem {") || !code.contains(".execute(") {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these hold a wait before a deletion and draw it themselves; \
         raise Silica's own RemorseItem over the row instead, so it looks \
         like every other countdown on the phone:\n  {}",
        offenders.join("\n  ")
    );
}

/// Drawn countdown uses list's number, never own.
///
/// Row's `RemorseItem` and list's delete are two clocks: must end together. Else countdown
/// ran out, platform put message back while list still waited. Only
/// `PendingRemoval.countdownFor(id)` (what is left of that id's wait) may go to `execute`.
/// Counted (as many `countdownFor(` as `.execute(`): call wraps.
#[test]
fn the_drawn_countdown_is_asked_for_rather_than_chosen() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        if !code.contains("PendingRemoval {") {
            continue;
        }
        let raised = code.matches(".execute(").count();
        let asked = code.matches("countdownFor(").count();
        if raised != asked {
            offenders.push(format!(
                "{}: {raised} countdown(s) raised, {asked} asked for",
                file.display()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "these raise a countdown with a length of their own choosing; \
         pass PendingRemoval.countdownFor(id) instead, so what is drawn \
         and what is deleted end together:\n  {}",
        offenders.join("\n  ")
    );
}

/// Waiting row is faded, not hidden.
///
/// Hiding takes children's `visible`; row parts measure `visible ? implicitHeight : 0`, so
/// row collapses and its replacement draws over neighbours (three "Deleting" stacked on
/// phone). `opacity` keeps height; `enabled` stops taps on unseen controls.
///
/// One line at a time: wrapped binding escapes. Five files: counted by
/// `every_pending_removal_is_emptied_on_the_way_out`.
#[test]
fn a_row_waiting_to_go_is_faded_rather_than_hidden() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        if !code.contains("PendingRemoval {") {
            continue;
        }
        for (number, line) in code.lines().enumerate() {
            let line = line.trim();
            let hides =
                line.contains('!') && (line.contains("doomed") || line.contains(".pending("));
            if line.starts_with("visible:") && hides {
                offenders.push(format!("{}:{}: {line}", file.display(), number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these hide a row's content while it waits to go, which collapses \
         the row and draws what replaces it over the neighbours; fade it \
         with `opacity` and take its taps with `enabled` \
         instead:\n  {}",
        offenders.join("\n  ")
    );
}

/// Every wait emptied when its holder is left.
///
/// Unemptied wait = deletion reader asked for, never got (left within the wait). Pages
/// flush on deactivate, as `ConversationPage` writes its draft.
/// `ConversationList` offers `flushDeletes()`; checked by
/// `the_conversation_page_uses_the_pieces_that_are_tested`.
#[test]
fn every_pending_removal_is_emptied_on_the_way_out() {
    let pages = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages");
    let mut holders = 0;
    let mut offenders = Vec::new();
    for file in qml_files() {
        let text = fs::read_to_string(&file).expect("read qml");
        if !code_only(&text).contains("PendingRemoval {") {
            continue;
        }
        holders += 1;
        let leaving = if file.parent() == Some(pages.as_path()) {
            block_of(&text, "PageStatus.Deactivating")
        } else {
            text.clone()
        };
        if !leaving.contains(".flush()") {
            offenders.push(file.display().to_string());
        }
    }
    // Else passes by finding nothing.
    assert!(
        holders >= 5,
        "only {holders} files hold a PendingRemoval; there should be five \
         -- the conversation, the chat list, the profiles, a group's \
         members and a chat's media"
    );
    assert!(
        offenders.is_empty(),
        "these hold a wait before a deletion and never empty it, so \
         leaving inside the wait keeps what the reader asked to \
         delete:\n  {}",
        offenders.join("\n  ")
    );
}

/// Path from other end becomes URL one segment at a time.
///
/// `encodeURI` keeps `#`/`?`: `a#b.png` cut at `a`. Per-segment `encodeURIComponent` fixes.
#[test]
fn file_urls_are_encoded_per_segment() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        for (number, line) in code.lines().enumerate() {
            if line.contains("encodeURI(") {
                offenders.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "encodeURI() keeps '#' and '?' as URL syntax, so a file named with \
         either points at a different URL. Build the path with \
         path.split('/').map(encodeURIComponent).join('/') instead, as \
         AttachmentPreview.qml does:\n  {}",
        offenders.join("\n  ")
    );
}

/// Only picker pages name `Sailfish.Pickers`: missing type takes whole file down. Kept to
/// dedicated files (pushed by URL), cost is one button, not a page.
/// Named one by one: adding one is a decision.
#[test]
fn only_the_picker_pages_import_sailfish_pickers() {
    const ALLOWED: [&str; 4] = [
        "AttachAppPage.qml",
        "AttachLibraryPage.qml",
        "AttachPhotoPage.qml",
        "BackupFilePage.qml",
    ];
    let mut offenders = Vec::new();
    for file in qml_files() {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        let imports_pickers = code
            .lines()
            .any(|line| line.trim_start().starts_with("import Sailfish.Pickers"));
        if imports_pickers && !ALLOWED.contains(&name.as_str()) {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these import Sailfish.Pickers outside {ALLOWED:?}, so a picker type \
         that is missing takes the whole page down rather than one button; \
         push the picker page by URL and connect to its `picked` signal, as \
         SettingsPage.pickPicture does:\n  {}",
        offenders.join("\n  ")
    );
}

/// Only webxdc pages and call view name `Sailfish.WebView`: engine is a separate package;
/// missing, it takes down files naming it. These are pushed/loaded apart, so chats still
/// open and a ringing call can still be seen and declined.
#[test]
fn only_the_webxdc_pages_import_sailfish_webview() {
    const ALLOWED: [&str; 3] = ["WebxdcPage.qml", "WebxdcStorePage.qml", "CallView.qml"];
    let mut offenders = Vec::new();
    for file in qml_files() {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        let imports_webview = code
            .lines()
            .any(|line| line.trim_start().starts_with("import Sailfish.WebView"));
        if imports_webview && !ALLOWED.contains(&name.as_str()) {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these import Sailfish.WebView outside {ALLOWED:?}, so a missing \
         browser engine takes the whole page down rather than the one app it \
         runs; push the page by URL instead, as ConversationPage.openApp \
         does:\n  {}",
        offenders.join("\n  ")
    );
}

/// Webxdc page must not set its `WebView`'s `active`.
///
/// `WebView.qml` binds it to page status and app-in-front (when engine renders, returns
/// GPU); override leaves view a grey rectangle (first device build).
#[test]
fn the_webxdc_pages_leave_the_views_activation_alone() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages");
    for name in ["WebxdcPage.qml", "WebxdcStorePage.qml"] {
        let code = code_only(&fs::read_to_string(root.join(name)).expect("read the page"));
        let offender = code
            .lines()
            .enumerate()
            .find(|(_, line)| line.trim_start().starts_with("active:"));
        assert!(
            offender.is_none(),
            "{name} sets `active` (line {}), which overrides the binding \
             Sailfish's own WebView.qml makes on it; the view then does not \
             follow the page's status and draws grey",
            offender.map_or(0, |(number, _)| number + 1)
        );
    }
}

/// Call view holds its `WebView` active for the whole call.
///
/// Opposite of rule above: inactive = hidden document, engine pauses its media (other end's
/// voice). Before page up, follows `shown` (what `WebView.qml` follows): never activated
/// by page transition, view draws nothing.
#[test]
fn the_call_view_holds_its_page_running_only_once_it_has_come_up() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components/CallView.qml");
    let code = code_only(&fs::read_to_string(file).expect("read the call view"));
    let active: Vec<&str> = code
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("active:"))
        .collect();
    assert_eq!(
        active,
        vec!["active: root.shown || (root.holding && view.loaded)"],
        "the call view's `active` is not the page's own condition, held \
         once the page has come up"
    );
}

/// Share methods in desktop file must match `ShareProvider`s in app.
///
/// Mismatch = dead sheet entry or unreached provider, phone only.
/// Sheet reads descriptions from desktop file, which no catalogue reaches: repeated in
/// `ShareTarget.qml` under `qsTr` for `lupdate`, and must match.
#[test]
fn the_share_methods_and_what_answers_them_agree() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let desktop =
        fs::read_to_string(root.join("harbour-piirit.desktop")).expect("read the desktop entry");
    let target =
        fs::read_to_string(root.join("qml/share/ShareTarget.qml")).expect("read the share target");

    // Desktop entry side: list, and group per name.
    let offered: Vec<String> = desktop
        .lines()
        .find_map(|line| line.trim().strip_prefix("X-Share-Methods="))
        .map(|value| {
            value
                .split(';')
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !offered.is_empty(),
        "the desktop entry offers no share methods, so the app is not in \
         the phone's share sheet at all"
    );

    // App side: one provider per method. Names are literals, which `code_only` blanks.
    let answered: Vec<String> = code_only_keeping_strings(&target)
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("method:")
                .map(|value| value.trim().trim_matches('"').to_string())
        })
        .collect();
    for name in &offered {
        assert!(
            answered.contains(name),
            "the desktop entry offers the share method {name:?} and no \
             ShareProvider answers it: {answered:?}"
        );
        let group = format!("[X-Share Method {name}]");
        let description = block_of(&desktop, &group)
            .lines()
            .find_map(|line| line.trim().strip_prefix("Description="))
            .map(str::to_string)
            .unwrap_or_default();
        assert!(
            !description.is_empty(),
            "the share method {name:?} has no {group} group with a \
             Description= in it, so the sheet has nothing to call it"
        );
        assert!(
            target.contains(&format!("qsTr(\"{description}\")")),
            "the share method {name:?} is called {description:?} in the \
             desktop entry and that string is not under qsTr in \
             ShareTarget.qml, so no catalogue carries it"
        );
    }
    for name in &answered {
        assert!(
            offered.contains(name),
            "a ShareProvider answers {name:?}, which the desktop entry does \
             not offer, so nothing will ever reach it: {offered:?}"
        );
    }
}

/// `Sailfish.Share` only under `qml/share/`: missing module fails file. Those
/// files only from a `Loader` (window's share target, viewer's share sheet):
/// missing module costs sharing, not app. So no page imports that directory
/// as types either.
#[test]
fn only_the_share_files_import_sailfish_share() {
    let mut offenders = Vec::new();
    for file in qml_files() {
        let in_share = file
            .parent()
            .and_then(|dir| dir.file_name())
            .is_some_and(|dir| dir == "share");
        let code = code_only_keeping_strings(&fs::read_to_string(&file).expect("read qml"));
        let imports_module = code
            .lines()
            .any(|line| line.trim_start().starts_with("import Sailfish.Share"));
        let imports_directory = code.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("import \"") && line.contains("share\"")
        });
        if (imports_module && !in_share) || imports_directory {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these import Sailfish.Share outside qml/share/, or that directory \
         as types, so a release without the module takes them down with it; \
         load the share file from a Loader instead, as piirit.qml does:\n  {}",
        offenders.join("\n  ")
    );
}

/// Every frame script a page loads is a shipped file.
///
/// `loadFrameScript` ignores missing files: store silently stops catching taps, on phone
/// only. Stub view doesn't look, so path checked here against package tree.
#[test]
fn the_frame_scripts_the_pages_load_are_there() {
    let mut loaded = Vec::new();
    for file in qml_files() {
        let code = code_only_keeping_strings(&fs::read_to_string(&file).expect("read qml"));
        for line in code.lines() {
            let Some(rest) = line.split_once("loadFrameScript(") else {
                continue;
            };
            let Some(path) = rest.1.split('"').nth(1) else {
                panic!(
                    "{}: loadFrameScript with no path in it: {line}",
                    file.display()
                );
            };
            // `Qt.resolvedUrl`: resolves against naming file; one dir up per `../`.
            let mut here = file.parent().expect("a file is in a directory");
            let mut rest = path;
            while let Some(above) = rest.strip_prefix("../") {
                here = here.parent().expect("qml/ has a parent");
                rest = above;
            }
            let script = here.join(rest.trim_start_matches("./"));
            assert!(
                script.is_file(),
                "{} loads the frame script {path}, which is not in the tree \
                 at {}",
                file.display(),
                script.display()
            );
            loaded.push(path.to_string());
        }
    }
    assert!(
        loaded.contains(&"../webxdc/catch.js".to_string()),
        "the store no longer loads the frame script that catches a tap on \
         an app; if that moved, this list moves with it: {loaded:?}"
    );
}

/// File app hands over is kept, not put into a chat.
///
/// Call is `sendToChat`, but app's button is a download: reader means the file. Saved where
/// file manager looks, no chooser. Page names `Sailfish.WebView`: only checked here.
#[test]
fn the_webxdc_page_keeps_a_handed_over_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/WebxdcPage.qml");
    let text = fs::read_to_string(&path).expect("read WebxdcPage.qml");
    let code = code_only(&text);
    assert!(
        code.contains("onHanded_over:"),
        "the page ignores an app handing a file over, so the call resolves \
         and nothing happens"
    );
    assert!(
        !code.contains("appWindow.shareInto("),
        "a file an app handed over still goes to the chat picker, which \
         is not what a download means"
    );
    let offer = block_of(&code, "function offer(");
    assert!(
        offer.contains("handoverSaver.keep("),
        "the file is not kept anywhere the reader can find it: {offer:?}"
    );
    assert!(
        !text.contains("HandoverDialog"),
        "the page still asks what to do with the file rather than keeping it"
    );
    // Page deletes cache copy: nothing else knows save happened; leftover doubles storage.
    assert!(
        offer.contains("handoverSaver.handedOver ="),
        "the page does not keep the path it is saving from, so it has \
         nothing to delete afterwards: {offer:?}"
    );
    assert!(
        code.contains("app.discard("),
        "the copy in the cache is never deleted, so every file an app \
         hands over stays on the phone twice"
    );
}

/// Every kept copy goes to one folder.
///
/// Only `AttachmentSaver` makes a `FileSaver` (else per-page folders); `keep` takes no folder.
#[test]
fn every_copy_goes_to_the_one_folder() {
    let mut makers = Vec::new();
    let mut keepers = 0;
    let mut offenders = Vec::new();
    for file in qml_files() {
        let code = code_only(&fs::read_to_string(&file).expect("read qml"));
        if code.contains("FileSaver {") {
            makers.push(
                file.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        }
        if !code.contains("AttachmentSaver {") {
            continue;
        }
        keepers += 1;
        if code.contains(".save(") || code.contains(".save_as(") {
            offenders.push(file.display().to_string());
        }
    }
    assert_eq!(
        makers,
        ["AttachmentSaver.qml"],
        "a FileSaver is made somewhere other than AttachmentSaver, so a \
         copy can go to a folder of its own"
    );
    // Else passes by finding nothing.
    assert!(
        keepers >= 5,
        "only {keepers} files keep a copy through AttachmentSaver; there \
         should be five -- the conversation, a picture, a video, a chat's \
         media and a webxdc app's download"
    );
    assert!(
        offenders.is_empty(),
        "these hand AttachmentSaver a folder rather than asking it to \
         `keep` a file:\n  {}",
        offenders.join("\n  ")
    );
}

/// Covered page must not stop its app.
///
/// Handover dialog pushed over app; stopping on deactivate kills the request that asked
/// (closed socket). Stop on destruction (pop, stack replace). Page names `Sailfish.WebView`:
/// only checked here.
#[test]
fn the_webxdc_page_keeps_serving_an_app_it_has_opened_a_page_over() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/WebxdcPage.qml");
    let text = fs::read_to_string(&path).expect("read WebxdcPage.qml");
    let code = code_only(&text);
    assert!(
        !code.contains("PageStatus.Deactivating"),
        "the page stops its app when it deactivates, which is what being \
         covered by the handover dialog is: handing a file over opens \
         that dialog, so asking to export one kills the request that asked"
    );
    assert!(
        code.contains("Component.onDestruction:")
            && block_of(&code, "Component.onDestruction:").contains("app.stop()"),
        "nothing stops the app when the page goes, so an app the reader \
         has left carries on being served"
    );
}

/// Every page the app pushes exists.
///
/// Name resolved at tap: wrong name = tap does nothing. Device-only pages never loaded by a
/// test, so names checked here.
#[test]
fn the_pages_the_app_pushes_are_there() {
    let mut pushed = Vec::new();
    for file in qml_files() {
        let code = code_only_keeping_strings(&fs::read_to_string(&file).expect("read qml"));
        for line in code.lines() {
            let Some(rest) = line.split_once("Qt.resolvedUrl(") else {
                continue;
            };
            // Literal paths only: variable-built ones can't be followed.
            // `Qt.resolvedUrl("file://" + path)` is a phone file, not ours.
            let Some(path) = rest.1.split('"').nth(1) else {
                continue;
            };
            let kind = Path::new(path)
                .extension()
                .map(|kind| kind.to_string_lossy().into_owned())
                .unwrap_or_default();
            if kind != "qml" && kind != "js" {
                continue;
            }
            // `Qt.resolvedUrl` resolves against naming file; see frame scripts above.
            let mut here = file.parent().expect("a file is in a directory");
            let mut rest = path;
            while let Some(above) = rest.strip_prefix("../") {
                here = here.parent().expect("qml/ has a parent");
                rest = above;
            }
            let target = here.join(rest.trim_start_matches("./"));
            assert!(
                target.is_file(),
                "{} pushes {path}, which is not in the tree at {}",
                file.display(),
                target.display()
            );
            pushed.push(path.to_string());
        }
    }
    assert!(
        pushed.iter().any(|path| path.ends_with("MessagePage.qml")),
        "nothing pushes MessagePage.qml any more, so a long message has \
         nowhere to be read whole: {pushed:?}"
    );
    assert!(
        pushed
            .iter()
            .any(|path| path.ends_with("DeleteMessageDialog.qml")),
        "nothing pushes DeleteMessageDialog.qml any more, so Delete has \
         nowhere to ask which kind of delete it is: {pushed:?}"
    );
}

/// Comments blanked, strings kept. Scanned paths are string literals.
fn code_only_keeping_strings(text: &str) -> String {
    text.lines()
        .map(|line| match line.split_once("//") {
            // `//` in string is not comment; only URLs here.
            Some((code, _)) if !code.contains('"') => code,
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Comments and string bodies blanked, newlines kept.
/// Else a URL in a string ends a line comment; quotes pair wrong.
fn code_only(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let here = chars[i];
        let next = chars.get(i + 1).copied();
        if here == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if here == '/' && next == Some('*') {
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    out.push_str("  ");
                    i += 2;
                    break;
                }
                out.push(blank(chars[i]));
                i += 1;
            }
        } else if here == '"' || here == '\'' {
            out.push(' ');
            i += 1;
            while i < chars.len() && chars[i] != here {
                if chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < chars.len() {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            if i < chars.len() {
                out.push(' ');
                i += 1;
            }
        } else {
            out.push(here);
            i += 1;
        }
    }
    out
}

/// Identifiers and punctuation, in order.
fn tokens(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in code.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            out.push(c.to_string());
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Every name file introduces: ids, properties, functions, params, `var`s.
///
/// Greedy: extra name harmless; missing declaration breaks check.
fn declared_names(code: &str) -> std::collections::HashSet<String> {
    fn is_name(token: &str) -> bool {
        token
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    }

    let tokens = tokens(code);
    let mut names = std::collections::HashSet::new();
    let mut i = 0;
    while i < tokens.len() {
        // Tokens after keyword, up to end of declaration.
        let (skip, stop): (usize, &[&str]) = match tokens[i].as_str() {
            "id" if tokens.get(i + 1).map(String::as_str) == Some(":") => (2, &[]),
            "property" => (1, &[":", "{", "}"]),
            "function" | "signal" => (1, &[")", "{"]),
            "var" => (1, &["="]),
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + skip;
        // Bare `id:` or `var` names one thing.
        let single = stop.is_empty() || tokens[i] == "var";
        while j < tokens.len() && !stop.contains(&tokens[j].as_str()) {
            if is_name(&tokens[j]) {
                names.insert(tokens[j].clone());
                if single {
                    break;
                }
            }
            j += 1;
        }
        i = j.max(i + 1);
    }
    names
}

/// Every `foo.` in code, with its line.
fn qualified_uses(code: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = code.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = Vec::new();
    for (at, c) in chars.iter().enumerate() {
        if *c != '.' {
            continue;
        }
        let mut end = at;
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        if start == end {
            continue;
        }
        // `a.b.c` reads `b` off `a`; only chain head must exist.
        if start > 0 && (chars[start - 1] == '.' || is_word(chars[start - 1])) {
            continue;
        }
        let name: String = chars[start..end].iter().collect();
        // Types, attached props, enums capitalised; numbers not names.
        if !name.starts_with(|c: char| c.is_lowercase() && c.is_alphabetic()) {
            continue;
        }
        let line = chars[..start].iter().filter(|c| **c == '\n').count() + 1;
        out.push((line, name));
    }
    out
}

/// Name read as `<name>.something` must be declared in file or QML scope.
///
/// Missing id: no load error; binding throws, fed value keeps default. `SearchResultRow`
/// lost `Avatar` but kept references: rows collapsed, "Chats (1)" over nothing.
#[test]
fn qml_reads_no_name_that_is_not_there() {
    // In scope without declaring.
    const IN_SCOPE: [&str; 26] = [
        // Grouped props and configured element's props, unqualified.
        "anchors",
        "font",
        "icon",
        "text",
        "parent",
        // MouseArea drag group.
        "drag",
        // Rectangle outline.
        "border",
        // Camera metaData group (file contents).
        "metaData",
        // Camera flash and imageProcessing groups.
        "flash",
        "imageProcessing",
        // Animation easing curve.
        "easing",
        // Item layer and its effect (Avatar desaturates).
        "layer",
        // Image, AnimatedImage, Nemo Thumbnail: decode size.
        "sourceSize",
        // Silica item colours (`palette.colorScheme`); GalleryPage.
        "palette",
        // PageHeader's interior (`extraContent.children`); GalleryPage's Back.
        "extraContent",
        // PinchArea `pinch` group; PinchEvent to handlers.
        "pinch",
        // MouseEvent to MouseArea handlers.
        "mouse",
        // Delegate scope.
        "model",
        "modelData",
        "section",
        "index",
        // Set from Rust (main.rs), and Silica's own.
        "core",
        "pageStack",
        // Root window id from piirit.qml. Test-loaded page has none: ChatListPage uses typeof.
        "appWindow",
        // ContentPickerPage answer; see AttachPhotoPage.
        "selectedContentProperties",
        // WebView `recvAsyncMessage` payload; see WebxdcStorePage.
        "data",
    ];

    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let code = code_only(&fs::read_to_string(file).expect("read qml"));
        let declared = declared_names(&code);
        let mut seen = Vec::new();
        for (line, name) in qualified_uses(&code) {
            if declared.contains(&name) || IN_SCOPE.contains(&name.as_str()) {
                continue;
            }
            if seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            offenders.push(format!("{}:{line}: {name}.…", file.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "these read something off a name the file never declares. If it was \
         an id, the element is gone and the references were left behind: the \
         binding throws at load and silently keeps its default, which is how \
         a row ends up with no height. If it is something QML puts in scope, \
         add it to IN_SCOPE with a note saying where it comes from.\n  {}",
        offenders.join("\n  ")
    );
}

/// QML text minus comments, so a comment saying why unused is not a use.
fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut in_block = false;
    for line in text.lines() {
        let mut rest = line;
        let mut kept = String::new();
        loop {
            if in_block {
                match rest.find("*/") {
                    Some(end) => {
                        rest = &rest[end + 2..];
                        in_block = false;
                    }
                    None => break,
                }
            } else if let Some(start) = rest.find("/*") {
                kept.push_str(&rest[..start]);
                rest = &rest[start + 2..];
                in_block = true;
            } else {
                kept.push_str(rest);
                break;
            }
        }
        if !kept.trim_start().starts_with("//") {
            out.push_str(&kept);
        }
        out.push('\n');
    }
    out
}

/// Whether `line` has `word` as own word.
fn names(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        !line[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
    })
}

/// No QML draws through its own shader.
///
/// Sailfish Qt 5.6 deletes `ShaderEffect` material types on window hide while programs keyed
/// by type address live on: later effect may draw with another's program. Cover faces came
/// out white (`qml_avatar_shaders.rs`). `layer` effects and `QtGraphicalEffects` count too.
/// Make pictures in image provider (`image://piirit/`, `src/pictures.rs`).
#[test]
fn qml_draws_nothing_through_a_shader_of_its_own() {
    let files = qml_files();
    assert!(!files.is_empty(), "found no .qml files to check");

    let mut offenders = Vec::new();
    for file in &files {
        let text = without_comments(&fs::read_to_string(file).expect("read qml"));
        for (number, line) in text.lines().enumerate() {
            if ["ShaderEffect", "layer.", "layer {", "QtGraphicalEffects"]
                .iter()
                .any(|banned| names(line, banned))
            {
                offenders.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a ShaderEffect, a layer or a QtGraphicalEffects effect can be drawn \
         with another effect's program on Qt 5.6 once its window has been \
         hidden. Make the picture as it is to be seen in src/pictures.rs \
         and draw it with a plain Image instead.\n  {}",
        offenders.join("\n  ")
    );
}
