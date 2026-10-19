//! Fields to be extracted, and what is found for scan.

use std::ops::Range;

/// The fields that scan prints, in this order.
pub struct FieldSet {
    names: Vec<Vec<u8>>,
    /// For `References:`, only the last line is extracted.
    references: Option<usize>,
}

impl FieldSet {
    pub const DEFAULT: &[u8] = b"Subject:,Date:,From:,To:,Cc:,Content-Type:,Content-Transfer-Encoding:,X-Mew-UIDL:,Message-ID:,In-Reply-To:,References:,X-Mew-Ref:";

    pub const REFERENCES: &[u8] = b"References:";

    /// `names` is comma-separated.
    pub fn parse(names: &[u8]) -> Self {
        let names: Vec<Vec<u8>> = names.split(|&c| c == b',').map(<[u8]>::to_vec).collect();
        let references = names.iter().rposition(|n| n == Self::REFERENCES);
        FieldSet { names, references }
    }

    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }

    pub fn is_references(&self, i: usize) -> bool {
        self.references == Some(i)
    }
}

/// Fields and the first lines of the body found by
/// [`super::Header::summarize`], as ranges of the header buffer.
pub struct Summary {
    /// What is printed for each field of [`FieldSet`], if found.
    pub fields: Vec<Option<Range<usize>>>,
    pub body: Vec<BodyLine>,
}

pub struct BodyLine {
    pub range: Range<usize>,
    /// A line beginning with `.` is escaped as `..`.
    pub dot_stuffed: bool,
}
