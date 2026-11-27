//! The lattice index: a sparse coordinate → byte-span table built
//! from one scan of a phext buffer, then binary-searched (O(log n)).
//!
//! This is phext-lattice's `LatticeIndex` design, homed here as of
//! holo-phext 0.2.0 — phext-lattice now re-exports this type instead
//! of carrying its own copy. Sorted arrays (not a hash map) because
//! spans load straight from a persisted table, coordinates come out
//! in document order for free, and the table is 88 bytes per scroll.
//!
//! One deliberate difference from the phext-lattice original:
//! `dimension_extent` takes this module's own `Dimension` enum
//! instead of phext-lattice's `coordinate_ext::Dimension`, so the
//! type has no dependency on its consumer.

use libphext::phext::{default_coordinate, Coordinate};

/// A scroll's location within the raw phext byte buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollSpan {
    /// Byte offset where this scroll's content begins (inclusive).
    pub start: usize,
    /// Byte offset where this scroll's content ends (exclusive).
    pub end: usize,
}

impl ScrollSpan {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// The nine phext dimensions, in address order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Library,
    Shelf,
    Series,
    Collection,
    Volume,
    Book,
    Chapter,
    Section,
    Scroll,
}

/// The value of one dimension of a coordinate.
pub fn dimension_value(coord: &Coordinate, dim: Dimension) -> usize {
    match dim {
        Dimension::Library => coord.z.library,
        Dimension::Shelf => coord.z.shelf,
        Dimension::Series => coord.z.series,
        Dimension::Collection => coord.y.collection,
        Dimension::Volume => coord.y.volume,
        Dimension::Book => coord.y.book,
        Dimension::Chapter => coord.x.chapter,
        Dimension::Section => coord.x.section,
        Dimension::Scroll => coord.x.scroll,
    }
}

/// The lattice index: maps every populated coordinate to its byte span.
#[derive(Debug, Clone, Default)]
pub struct LatticeIndex {
    /// All populated coordinates in document order (= sorted order).
    ordered: Vec<Coordinate>,
    /// `spans[i]` is the byte span of `ordered[i]`.
    spans: Vec<ScrollSpan>,
    /// Total size of the indexed buffer in bytes.
    buffer_len: usize,
}

#[derive(Debug, Clone, Copy)]
enum DimBreak {
    Scroll,
    Section,
    Chapter,
    Book,
    Volume,
    Collection,
    Series,
    Shelf,
    Library,
}

fn dim_break(byte: u8) -> Option<DimBreak> {
    match byte {
        0x17 => Some(DimBreak::Scroll),
        0x18 => Some(DimBreak::Section),
        0x19 => Some(DimBreak::Chapter),
        0x1A => Some(DimBreak::Book),
        0x1C => Some(DimBreak::Volume),
        0x1D => Some(DimBreak::Collection),
        0x1E => Some(DimBreak::Series),
        0x1F => Some(DimBreak::Shelf),
        0x01 => Some(DimBreak::Library),
        _ => None,
    }
}

impl LatticeIndex {
    /// Build an index by scanning the raw phext bytes once.
    /// Time: O(n) in buffer length. Space: O(k) in populated scrolls.
    /// Empty runs between delimiter bytes are addressing, not
    /// scrolls, and get no span (a scroll exists iff it holds at
    /// least one byte — the phext definition).
    pub fn build(buffer: &[u8]) -> LatticeIndex {
        let mut entries: Vec<(Coordinate, ScrollSpan)> = Vec::new();
        let mut coord = default_coordinate();
        let mut scroll_start: usize = 0;
        let len = buffer.len();

        for (i, &byte) in buffer.iter().enumerate() {
            if let Some(brk) = dim_break(byte) {
                let span = ScrollSpan { start: scroll_start, end: i };
                if !span.is_empty() {
                    entries.push((coord, span));
                }
                match brk {
                    DimBreak::Scroll => coord.scroll_break(),
                    DimBreak::Section => coord.section_break(),
                    DimBreak::Chapter => coord.chapter_break(),
                    DimBreak::Book => coord.book_break(),
                    DimBreak::Volume => coord.volume_break(),
                    DimBreak::Collection => coord.collection_break(),
                    DimBreak::Series => coord.series_break(),
                    DimBreak::Shelf => coord.shelf_break(),
                    DimBreak::Library => coord.library_break(),
                }
                scroll_start = i + 1;
            }
        }

        if scroll_start < len {
            let span = ScrollSpan { start: scroll_start, end: len };
            if !span.is_empty() {
                entries.push((coord, span));
            }
        }

        LatticeIndex::from_entries(entries, len)
    }

    /// Build from (coordinate, span) pairs, e.g. a persisted span
    /// table or spans recorded while serializing. Pairs are normally
    /// already in document order; otherwise they are sorted. A
    /// repeated coordinate keeps its last span.
    pub fn from_entries(mut entries: Vec<(Coordinate, ScrollSpan)>, buffer_len: usize) -> LatticeIndex {
        if !entries.windows(2).all(|w| w[0].0 < w[1].0) {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut deduped: Vec<(Coordinate, ScrollSpan)> = Vec::with_capacity(entries.len());
            for e in entries {
                match deduped.last_mut() {
                    Some(last) if last.0 == e.0 => *last = e,
                    _ => deduped.push(e),
                }
            }
            entries = deduped;
        }
        let (ordered, spans) = entries.into_iter().unzip();
        LatticeIndex { ordered, spans, buffer_len }
    }

    /// Look up the byte span for a coordinate. O(log n).
    pub fn get(&self, coord: &Coordinate) -> Option<&ScrollSpan> {
        self.ordered.binary_search(coord).ok().map(|i| &self.spans[i])
    }

    /// Check if a coordinate has content.
    pub fn contains(&self, coord: &Coordinate) -> bool {
        self.ordered.binary_search(coord).is_ok()
    }

    /// Number of populated scrolls.
    pub fn scroll_count(&self) -> usize {
        self.ordered.len()
    }

    /// Spans in document order, parallel to `coordinates()`.
    pub fn spans(&self) -> &[ScrollSpan] {
        &self.spans
    }

    /// Total buffer size.
    pub fn buffer_len(&self) -> usize {
        self.buffer_len
    }

    /// All populated coordinates in document order.
    pub fn coordinates(&self) -> &[Coordinate] {
        &self.ordered
    }

    /// The next populated coordinate after `coord`, if any.
    pub fn next_populated(&self, coord: &Coordinate) -> Option<Coordinate> {
        match self.ordered.binary_search(coord) {
            Ok(idx) => self.ordered.get(idx + 1).copied(),
            Err(idx) => self.ordered.get(idx).copied(),
        }
    }

    /// The previous populated coordinate before `coord`, if any.
    pub fn prev_populated(&self, coord: &Coordinate) -> Option<Coordinate> {
        match self.ordered.binary_search(coord) {
            Ok(idx) | Err(idx) => {
                if idx > 0 {
                    Some(self.ordered[idx - 1])
                } else {
                    None
                }
            }
        }
    }

    /// All populated coordinates matching a predicate — the lattice
    /// navigator's workhorse.
    pub fn coordinates_matching<F>(&self, predicate: F) -> Vec<Coordinate>
    where
        F: Fn(&Coordinate) -> bool,
    {
        self.ordered.iter().copied().filter(predicate).collect()
    }

    /// The distinct values a dimension takes across all populated
    /// coordinates, sorted.
    pub fn dimension_extent(&self, dim: Dimension) -> Vec<usize> {
        let mut values: Vec<usize> = self.ordered.iter().map(|c| dimension_value(c, dim)).collect();
        values.sort_unstable();
        values.dedup();
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libphext::phext::to_coordinate;

    fn make_test_phext() -> Vec<u8> {
        // "hello" at 1.1.1/1.1.1/1.1.1, "world" at 1.1.1/1.1.1/1.1.2, "deep" at 1.1.1/1.1.1/1.2.1
        let mut buf = Vec::new();
        buf.extend_from_slice(b"hello");
        buf.push(0x17); // scroll break
        buf.extend_from_slice(b"world");
        buf.push(0x18); // section break
        buf.extend_from_slice(b"deep");
        buf
    }

    #[test]
    fn build_and_lookup() {
        let phext = make_test_phext();
        let index = LatticeIndex::build(&phext);
        assert_eq!(index.scroll_count(), 3);
        let base = to_coordinate("1.1.1/1.1.1/1.1.1");
        let span = index.get(&base).unwrap();
        assert_eq!(&phext[span.start..span.end], b"hello");
        let scroll2 = to_coordinate("1.1.1/1.1.1/1.1.2");
        let span2 = index.get(&scroll2).unwrap();
        assert_eq!(&phext[span2.start..span2.end], b"world");
        let section2 = to_coordinate("1.1.1/1.1.1/1.2.1");
        let span3 = index.get(&section2).unwrap();
        assert_eq!(&phext[span3.start..span3.end], b"deep");
    }

    #[test]
    fn next_prev_populated() {
        let phext = make_test_phext();
        let index = LatticeIndex::build(&phext);
        let base = to_coordinate("1.1.1/1.1.1/1.1.1");
        let next = index.next_populated(&base).unwrap();
        assert_eq!(next, to_coordinate("1.1.1/1.1.1/1.1.2"));
        let prev = index.prev_populated(&next).unwrap();
        assert_eq!(prev, base);
    }

    #[test]
    fn empty_coordinate_returns_none() {
        let phext = make_test_phext();
        let index = LatticeIndex::build(&phext);
        let missing = to_coordinate("5.5.5/5.5.5/5.5.5");
        assert!(index.get(&missing).is_none());
    }

    #[test]
    fn from_entries_sorts_and_keeps_last_duplicate() {
        let a = to_coordinate("1.1.1/1.1.1/1.1.2");
        let b = to_coordinate("1.1.1/1.1.1/1.1.1");
        let idx = LatticeIndex::from_entries(
            vec![
                (a, ScrollSpan { start: 0, end: 1 }),
                (b, ScrollSpan { start: 2, end: 3 }),
                (a, ScrollSpan { start: 4, end: 5 }),
            ],
            5,
        );
        assert_eq!(idx.coordinates(), &[b, a]);
        assert_eq!(idx.get(&a), Some(&ScrollSpan { start: 4, end: 5 }));
        assert_eq!(idx.spans().len(), 2);
    }

    #[test]
    fn document_order_preserved() {
        let phext = make_test_phext();
        let index = LatticeIndex::build(&phext);
        let coords = index.coordinates();
        assert_eq!(coords.len(), 3);
        assert!(coords[0] < coords[1]);
        assert!(coords[1] < coords[2]);
    }

    #[test]
    fn dimension_extent_reports_distinct_values() {
        let phext = make_test_phext();
        let index = LatticeIndex::build(&phext);
        assert_eq!(index.dimension_extent(Dimension::Scroll), vec![1, 2]);
        assert_eq!(index.dimension_extent(Dimension::Section), vec![1, 2]);
        assert_eq!(index.dimension_extent(Dimension::Volume), vec![1]);
    }
}
