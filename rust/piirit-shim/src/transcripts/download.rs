//! Speech model download: `QNetworkAccessManager` (Harbour allows `Qt5Network`; Rust HTTP + TLS
//! stack would be new and large), streamed to file, SHA-256 (`QCryptographicHash`) as it lands.
//!
//! Resumes with HTTP `Range`; part hashed again first ([`Hasher::of_file`], off Qt thread).
//! Server ignoring range (200): starts over. Size, hash checked by caller.

// `cpp!` is FFI: `unsafe`. Allowed here only: docs/BUILDING.md.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::Path;

use cpp::cpp;
use qmetaobject::QString;

cpp! {{
    #include <QtCore/QByteArray>
    #include <QtCore/QCryptographicHash>
    #include <QtCore/QFile>
    #include <QtCore/QObject>
    #include <QtCore/QString>
    #include <QtCore/QUrl>
    #include <QtNetwork/QNetworkAccessManager>
    #include <QtNetwork/QNetworkReply>
    #include <QtNetwork/QNetworkRequest>

    // Qt thread only.
    struct PiiritFetch {
        QNetworkAccessManager *manager = nullptr;
        QNetworkReply *reply = nullptr;
        QFile *file = nullptr;
        QCryptographicHash *hash = nullptr;
        QMetaObject::Connection on_read;
        QMetaObject::Connection on_finish;
        // Bytes in file before request; now; expected in all.
        qint64 offset = 0;
        qint64 written = 0;
        qint64 total = 0;
        int permille = -1;
        bool checked = false;
        bool finishing = false;
        bool ended = false;
        QString error;
        void *user = nullptr;
    };

    // kind: 0 progress, 1 done (text = hex digest), 2 failed (text = why).
    static void piirit_fetch_report(PiiritFetch *f, int kind, const QString &text) {
        void *user = f->user;
        qint64 written = f->written;
        rust!(PiiritFetch_report [user: *const c_void as "void *", kind: i32 as "int",
                                  written: i64 as "qint64", text: &QString as "const QString &"] {
            // SAFETY: `user` is Fetch's report; Fetch disconnects before freeing it.
            let report = unsafe { &*user.cast::<Report>() };
            let event = event(kind, written, text);
            // No unwind into Qt.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| report(event))).is_err() {
                eprintln!("piirit: the model download's callback panicked");
            }
        });
    }

    // Stop with reason; `finished` reports it.
    static void piirit_fetch_fail(PiiritFetch *f, const QString &why) {
        if (f->error.isEmpty()) {
            f->error = why;
        }
        if (!f->finishing) {
            f->reply->abort();
        }
    }

    static void piirit_fetch_read(PiiritFetch *f) {
        if (f->ended || !f->error.isEmpty()) {
            return;
        }
        if (!f->checked) {
            f->checked = true;
            int status = f->reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            if (status == 200 && f->offset > 0) {
                // Range ignored: whole file follows.
                if (!f->file->resize(0)) {
                    piirit_fetch_fail(f, QStringLiteral("could not restart the model file: ") + f->file->errorString());
                    return;
                }
                f->hash->reset();
                f->offset = 0;
                f->written = 0;
            } else if (status != 200 && status != 206) {
                piirit_fetch_fail(f, QStringLiteral("the server answered %1").arg(status));
                return;
            }
        }
        QByteArray chunk = f->reply->readAll();
        if (chunk.isEmpty()) {
            return;
        }
        if (f->written + chunk.size() > f->total) {
            piirit_fetch_fail(f, QStringLiteral("the model is larger than expected"));
            return;
        }
        if (f->file->write(chunk) != chunk.size()) {
            piirit_fetch_fail(f, QStringLiteral("could not write the model: ") + f->file->errorString());
            return;
        }
        f->hash->addData(chunk);
        f->written += chunk.size();
        int permille = f->total > 0 ? int(f->written * 1000 / f->total) : 0;
        if (permille != f->permille) {
            f->permille = permille;
            piirit_fetch_report(f, 0, QString());
        }
    }

    static void piirit_fetch_finished(PiiritFetch *f) {
        if (f->ended) {
            return;
        }
        f->finishing = true;
        if (f->reply->error() == QNetworkReply::NoError) {
            piirit_fetch_read(f);
        }
        f->ended = true;
        bool flushed = f->file->flush();
        f->file->close();
        if (!f->error.isEmpty()) {
            piirit_fetch_report(f, 2, f->error);
        } else if (f->reply->error() != QNetworkReply::NoError) {
            piirit_fetch_report(f, 2, f->reply->errorString());
        } else if (!flushed) {
            piirit_fetch_report(f, 2, QStringLiteral("could not write the model: ") + f->file->errorString());
        } else {
            piirit_fetch_report(f, 1, QString::fromLatin1(f->hash->result().toHex()));
        }
    }

    // Takes `hash` (null: new). Null if file can't be opened.
    static PiiritFetch *piirit_fetch_start(const QString &url, const QString &path, qint64 offset,
                                           qint64 total, QCryptographicHash *hash, void *user) {
        PiiritFetch *f = new PiiritFetch();
        f->user = user;
        f->offset = offset;
        f->written = offset;
        f->total = total;
        f->hash = hash ? hash : new QCryptographicHash(QCryptographicHash::Sha256);
        f->file = new QFile(path);
        QIODevice::OpenMode mode = QIODevice::WriteOnly
            | (offset > 0 ? QIODevice::Append : QIODevice::Truncate);
        if (!f->file->open(mode)) {
            delete f->file;
            delete f->hash;
            delete f;
            return nullptr;
        }
        f->manager = new QNetworkAccessManager();
        QNetworkRequest request{QUrl(url)};
        request.setAttribute(QNetworkRequest::FollowRedirectsAttribute, true);
        if (offset > 0) {
            request.setRawHeader("Range", "bytes=" + QByteArray::number(offset) + "-");
        }
        f->reply = f->manager->get(request);
        f->on_read = QObject::connect(f->reply, &QIODevice::readyRead, f->reply,
                                      [f]() { piirit_fetch_read(f); });
        f->on_finish = QObject::connect(f->reply, &QNetworkReply::finished, f->reply,
                                        [f]() { piirit_fetch_finished(f); });
        return f;
    }

    // No callbacks after: disconnected before abort.
    static void piirit_fetch_stop(PiiritFetch *f) {
        QObject::disconnect(f->on_read);
        QObject::disconnect(f->on_finish);
        f->ended = true;
        f->reply->abort();
        f->reply->deleteLater();
        f->manager->deleteLater();
        f->file->close();
        delete f->file;
        delete f->hash;
        delete f;
    }

    // Any thread: plain object, no event loop.
    static QCryptographicHash *piirit_hash_file(const QString &path, qint64 *hashed) {
        QCryptographicHash *hash = new QCryptographicHash(QCryptographicHash::Sha256);
        QFile file(path);
        if (file.open(QIODevice::ReadOnly)) {
            for (;;) {
                QByteArray chunk = file.read(1 << 20);
                if (chunk.isEmpty()) {
                    break;
                }
                hash->addData(chunk);
                *hashed += chunk.size();
            }
        }
        return hash;
    }
}}

/// Download news, on Qt thread.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FetchEvent {
    /// Bytes in file now (permille moved).
    Progress(u64),
    /// Ended: hex SHA-256 of whole file, or why not.
    Done(Result<String, String>),
}

/// Called from inside Qt signal: must not drop [`Fetch`] there (pass a `queued_callback`).
type Report = Box<dyn Fn(FetchEvent)>;

fn event(kind: i32, written: i64, text: &QString) -> FetchEvent {
    match kind {
        0 => FetchEvent::Progress(u64::try_from(written).unwrap_or(0)),
        1 => FetchEvent::Done(Ok(text.to_string())),
        _ => FetchEvent::Done(Err(text.to_string())),
    }
}

/// SHA-256 so far of a file part; handed to [`Fetch::start`] to go on from.
pub(crate) struct Hasher {
    /// `QCryptographicHash`; 0 once handed over.
    handle: usize,
    bytes: u64,
}

impl Hasher {
    /// Hash `path` whole. Blocks for seconds on big file: not on Qt thread.
    pub(crate) fn of_file(path: &Path) -> Self {
        let path = QString::from(path.to_string_lossy().as_ref());
        let mut hashed: i64 = 0;
        let out = std::ptr::addr_of_mut!(hashed);
        // SAFETY: `out` outlives call; hash owned by result.
        let handle = cpp!(unsafe [path as "QString", out as "qint64 *"] -> *mut c_void as "void *" {
            return piirit_hash_file(path, out);
        });
        Self {
            handle: handle as usize,
            bytes: u64::try_from(hashed).unwrap_or(0),
        }
    }

    /// Bytes hashed.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Hex digest of bytes so far.
    pub(crate) fn hex(&self) -> String {
        let hash = self.handle as *mut c_void;
        // SAFETY: owned, not handed over.
        cpp!(unsafe [hash as "QCryptographicHash *"] -> QString as "QString" {
            return QString::fromLatin1(hash->result().toHex());
        })
        .to_string()
    }
}

impl Drop for Hasher {
    fn drop(&mut self) {
        let hash = std::mem::take(&mut self.handle) as *mut c_void;
        if hash.is_null() {
            return;
        }
        // SAFETY: owned; freed once.
        cpp!(unsafe [hash as "QCryptographicHash *"] {
            delete hash;
        });
    }
}

/// Download running. Drop stops it (file kept as far as written).
pub(crate) struct Fetch {
    /// `PiiritFetch`; never null.
    handle: usize,
    /// Read by C++ at its address; freed after handle.
    #[allow(dead_code)]
    report: Box<Report>,
}

impl Fetch {
    /// Fetch `url` into `path`: from `hash.bytes()` with `Range` if `hash` given (part kept),
    /// else from start. `total`: expected size. Qt thread.
    ///
    /// # Errors
    ///
    /// `path` can't be opened for writing.
    pub(crate) fn start(
        url: &str,
        path: &Path,
        total: u64,
        hash: Option<Hasher>,
        report: impl Fn(FetchEvent) + 'static,
    ) -> Result<Self, String> {
        let offset = hash
            .as_ref()
            .map_or(0, |hash| i64::try_from(hash.bytes).unwrap_or(0));
        let hash = hash.map_or(0, |mut hash| std::mem::take(&mut hash.handle)) as *mut c_void;
        let total = i64::try_from(total).unwrap_or(i64::MAX);
        let url = QString::from(url);
        let target = QString::from(path.to_string_lossy().as_ref());
        let report: Box<Report> = Box::new(Box::new(report));
        let user = std::ptr::addr_of!(*report).cast::<c_void>();
        // SAFETY: QStrings copied; `hash` taken over; `user` kept alive by `Self`.
        let handle = cpp!(unsafe [url as "QString", target as "QString", offset as "qint64",
                                  total as "qint64", hash as "QCryptographicHash *",
                                  user as "void *"] -> *mut c_void as "void *" {
            return piirit_fetch_start(url, target, offset, total, hash, user);
        });
        if handle.is_null() {
            return Err(format!("could not write {}", path.display()));
        }
        Ok(Self {
            handle: handle as usize,
            report,
        })
    }
}

impl Drop for Fetch {
    fn drop(&mut self) {
        let fetch = self.handle as *mut c_void;
        // SAFETY: made by `start`, stopped once here, before `report` drops.
        cpp!(unsafe [fetch as "PiiritFetch *"] {
            piirit_fetch_stop(fetch);
        });
    }
}
