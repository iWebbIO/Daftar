//! multipart/form-data, written by hand.
//!
//! `ureq` is built here without its multipart feature, so the body is framed
//! byte by byte: a boundary line, the part headers, a blank line, and then the
//! bytes exactly as they are on disk. Telegram parses this the same way every
//! other server does — nothing in the file is escaped, encoded or re-written,
//! which is what an invoice PDF needs.

use crate::db::now_ms;

/// A boundary that cannot occur inside a PDF or a zip: it is built from the
/// clock, so it differs on every upload and is far too long to appear by chance.
pub fn boundary() -> String {
    format!("----------daftar{}", now_ms())
}

/// One built request body, ready to hand to the transport.
pub struct Multipart {
    /// `multipart/form-data; boundary=…` — the value of the Content-Type header.
    pub content_type: String,
    /// The whole body: text parts, then the file, then the closing boundary.
    pub body: Vec<u8>,
}

/// Builds the body for one text-part list plus exactly one file.
pub fn build(
    boundary: &str,
    fields: &[(&str, &str)],
    file_field: &str,
    file_name: &str,
    bytes: &[u8],
) -> Multipart {
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 512);

    // The text parts: chat_id, caption, and anything else the method wants.
    for (name, value) in fields {
        push(
            &mut body,
            &format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n"),
        );
        push(&mut body, value);
        push(&mut body, "\r\n");
    }

    // The file part. The name goes in `filename=`; the type is a generic binary
    // stream because Telegram sniffs the real format from the bytes.
    push(
        &mut body,
        &format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"{file_field}\"; filename=\"{}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n",
            header_safe(file_name)
        ),
    );
    body.extend_from_slice(bytes);
    push(&mut body, "\r\n");

    // The end: the boundary again, this time with the two closing dashes.
    push(&mut body, &format!("--{boundary}--\r\n"));

    Multipart {
        content_type: format!("multipart/form-data; boundary={boundary}"),
        body,
    }
}

fn push(body: &mut Vec<u8>, text: &str) {
    body.extend_from_slice(text.as_bytes());
}

/// Keeps a file name from breaking the header it sits in. A quote or a line
/// break in a name would end the part early; those become underscores, and
/// everything else — Persian letters included — is left alone.
fn header_safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '"' | '\r' | '\n' | '\\' => '_',
            c => c,
        })
        .collect();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where `needle` first appears in `haystack`.
    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    #[test]
    fn multipart_frames_the_file_the_way_telegram_reads_it() {
        // Bytes that are not text and not UTF-8: nothing may touch them.
        let file: &[u8] = b"%PDF-1.7\r\n%\x00\xff\xfe binary \x01\x02\r\n%%EOF";
        let body = build(
            "BOUND",
            &[("chat_id", "12345"), ("caption", "فاکتور دکان رضا")],
            "document",
            "factor-1405-001.pdf",
            file,
        );

        assert_eq!(
            body.content_type,
            "multipart/form-data; boundary=BOUND"
        );

        // The framing: an opening boundary line and a closing one.
        assert!(body.body.starts_with(b"--BOUND\r\n"));
        assert!(body.body.ends_with(b"--BOUND--\r\n"));

        // Each part is announced by its own header, then a blank line, then data.
        let text = String::from_utf8_lossy(&body.body);
        assert!(text.contains("Content-Disposition: form-data; name=\"chat_id\"\r\n\r\n12345\r\n"));
        assert!(text.contains("Content-Disposition: form-data; name=\"caption\"\r\n\r\n"));
        assert!(text.contains("فاکتور دکان رضا"));

        // The file part names the file Telegram will show, and its field.
        assert!(text.contains(
            "Content-Disposition: form-data; name=\"document\"; filename=\"factor-1405-001.pdf\"\r\n"
        ));

        // The bytes appear verbatim, in one piece, right after the part header
        // and right before the closing boundary line.
        let at = find(&body.body, file).expect("the file bytes are in the body verbatim");
        assert!(body.body[..at].ends_with(b"\r\n\r\n"));
        assert!(body.body[at + file.len()..].starts_with(b"\r\n--BOUND--\r\n"));
    }

    #[test]
    fn a_file_name_cannot_break_the_header_it_sits_in() {
        let body = build("B", &[], "document", "we\"ird\r\nname.pdf", b"x");
        let text = String::from_utf8_lossy(&body.body);
        assert!(text.contains("filename=\"we_ird__name.pdf\""));
        // The boundary is long and built from the clock, so it is never empty
        // and never a word that could appear inside a document.
        let b = boundary();
        assert!(b.len() > 20, "boundary too short: {b}");
        assert!(b.starts_with("----------daftar"));
    }
}
