//! Sample Library browsing of the Synth OLED (Waveform page in Sample mode, manual p.29): the
//! CAT dial walks the categories of the opened backup, the SAMPLE dial the samples of the
//! category, and every change queues a load of the picked `.nsmp4` for the focused layer.
//!
//! The browser only knows names. It copies the sample entries of the backup catalog
//! (`Samp Lib/<category>/<name>.nsmp4`, the folder is the category) and hands the load requests
//! back to the bay, which forwards them as `LoadRequest::Sample` (the panel never touches the
//! archive). ASSUMED: categories are the archive folders, sorted by name; the hardware's own
//! category order and names are not decoded.

use crate::panel::bay_program::catalog::{category_of, display_name};

/// Number of Synth layers.
const LAYERS: usize = 3;

/// Catalog copy, per layer selection and the load requests waiting for the app.
#[derive(Debug, Clone, Default)]
pub struct SampleBrowser {
    entries: Vec<String>,
    /// Category names, sorted case-insensitively.
    cats: Vec<String>,
    /// Entry indices of each category, sorted by display name.
    by_cat: Vec<Vec<usize>>,
    /// Picked entry of each layer.
    sel: [Option<String>; LAYERS],
    /// `(layer, entry)`: at most one pending request per layer (the newest wins).
    requests: Vec<(usize, String)>,
    /// Last layer A sample name reported by the app (see `observe_loaded`).
    seen_loaded: Option<String>,
}

fn bump(v: usize, steps: i32, count: usize) -> usize {
    (v as i64 + i64::from(steps)).clamp(0, count.saturating_sub(1) as i64) as usize
}

impl SampleBrowser {
    /// Replaces the catalog (the `.nsmp4` entries of the backup); keeps picks that still exist.
    pub fn set_catalog(&mut self, samples: &[String]) {
        if self.entries == samples {
            return;
        }
        self.entries = samples.to_vec();
        let mut cats: Vec<String> = self.entries.iter().map(|e| category_of(e)).collect();
        cats.sort_by_key(|c| c.to_lowercase());
        cats.dedup();
        self.by_cat = cats
            .iter()
            .map(|c| {
                let mut idx: Vec<usize> = (0..self.entries.len())
                    .filter(|&i| category_of(&self.entries[i]) == *c)
                    .collect();
                idx.sort_by_key(|&i| display_name(&self.entries[i]).to_lowercase());
                idx
            })
            .collect();
        self.cats = cats;
        for s in &mut self.sel {
            if s.as_ref().is_some_and(|e| !self.entries.contains(e)) {
                *s = None;
            }
        }
    }

    /// No backup library (or no sample in it).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of categories.
    pub fn cat_count(&self) -> usize {
        self.cats.len()
    }

    /// `(category index, position in the category)` of the layer's pick.
    fn locate(&self, layer: usize) -> Option<(usize, usize)> {
        let entry = self.sel.get(layer)?.as_ref()?;
        let at = self.entries.iter().position(|e| e == entry)?;
        self.by_cat
            .iter()
            .enumerate()
            .find_map(|(c, list)| list.iter().position(|&i| i == at).map(|p| (c, p)))
    }

    /// Category name of the layer's pick.
    pub fn category(&self, layer: usize) -> Option<&str> {
        self.locate(layer).map(|(c, _)| self.cats[c].as_str())
    }

    /// `(1-based position, samples in the category)` of the layer's pick.
    pub fn position(&self, layer: usize) -> Option<(usize, usize)> {
        self.locate(layer)
            .map(|(c, p)| (p + 1, self.by_cat[c].len()))
    }

    /// Display name of the layer's pick.
    pub fn name(&self, layer: usize) -> Option<String> {
        let (c, p) = self.locate(layer)?;
        Some(display_name(&self.entries[self.by_cat[c][p]]))
    }

    /// Archive entry of the layer's pick.
    pub fn entry(&self, layer: usize) -> Option<&str> {
        self.sel.get(layer)?.as_deref()
    }

    /// Names above, at and below the pick (LIST view); empty strings outside the category.
    pub fn rows(&self, layer: usize) -> [String; 3] {
        let Some((c, p)) = self.locate(layer) else {
            return Default::default();
        };
        let at = |q: Option<usize>| {
            q.and_then(|q| self.by_cat[c].get(q))
                .map(|&i| display_name(&self.entries[i]))
                .unwrap_or_default()
        };
        [at(p.checked_sub(1)), at(Some(p)), at(Some(p + 1))]
    }

    /// Follows the sample the app reports as loaded into layer A (program load, drawer, or the
    /// result of this browser's own request): when the name changed, the layer's pick moves to the
    /// entry of that name unless the pick already has it. Unknown names change nothing.
    pub fn observe_loaded(&mut self, name: Option<&str>) {
        if self.seen_loaded.as_deref() == name {
            return;
        }
        self.seen_loaded = name.map(str::to_string);
        let Some(name) = name else { return };
        if self.name(0).as_deref() == Some(name) {
            return;
        }
        if let Some(e) = self.entries.iter().find(|e| display_name(e) == name) {
            self.sel[0] = Some(e.clone());
        }
    }

    fn pick(&mut self, layer: usize, c: usize, p: usize) {
        let entry = self.entries[self.by_cat[c][p]].clone();
        self.sel[layer] = Some(entry.clone());
        self.requests.retain(|(l, _)| *l != layer);
        self.requests.push((layer, entry));
    }

    /// CAT dial: moves to another category and picks its first sample.
    pub fn turn_cat(&mut self, layer: usize, steps: i32) {
        if self.is_empty() || layer >= LAYERS {
            return;
        }
        let cur = self.locate(layer).map(|(c, _)| c);
        let next = bump(cur.unwrap_or(0), steps, self.cats.len());
        if cur != Some(next) {
            self.pick(layer, next, 0);
        }
    }

    /// SAMPLE dial: moves within the category (the first click on an empty layer picks the first
    /// sample of the first category).
    pub fn turn_sample(&mut self, layer: usize, steps: i32) {
        if self.is_empty() || layer >= LAYERS {
            return;
        }
        match self.locate(layer) {
            Some((c, p)) => {
                let q = bump(p, steps, self.by_cat[c].len());
                if q != p {
                    self.pick(layer, c, q);
                }
            }
            None => {
                let q = bump(0, steps.max(1) - 1, self.by_cat[0].len());
                self.pick(layer, 0, q);
            }
        }
    }

    /// Load requests queued since the last call, as `(layer, archive entry)`.
    pub fn take_requests(&mut self) -> Vec<(usize, String)> {
        std::mem::take(&mut self.requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> SampleBrowser {
        let mut b = SampleBrowser::default();
        b.set_catalog(&[
            "Samp Lib/Strings/Viola.nsmp4".into(),
            "Samp Lib/Bass/Upright.nsmp4".into(),
            "Samp Lib/Strings/Cello.nsmp4".into(),
            "Samp Lib/Bass/Fretless.nsmp4".into(),
            "Samp Lib/Bass/Slap.nsmp4".into(),
        ]);
        b
    }

    #[test]
    fn categories_and_samples_are_sorted() {
        let b = lib();
        assert_eq!(b.cat_count(), 2);
        assert!(b.category(0).is_none());
        assert_eq!(b.rows(0), <[String; 3]>::default());
    }

    #[test]
    fn first_sample_click_picks_the_first_of_the_first_category() {
        let mut b = lib();
        b.turn_sample(1, 1);
        assert_eq!(b.category(1), Some("Bass"));
        assert_eq!(b.name(1).as_deref(), Some("Fretless"));
        assert_eq!(b.position(1), Some((1, 3)));
        assert_eq!(
            b.take_requests(),
            vec![(1, "Samp Lib/Bass/Fretless.nsmp4".to_string())]
        );
        assert!(b.category(0).is_none(), "other layers keep their state");
    }

    #[test]
    fn sample_dial_walks_within_the_category_and_clamps() {
        let mut b = lib();
        b.turn_sample(0, 1);
        b.turn_sample(0, 1);
        assert_eq!(b.name(0).as_deref(), Some("Slap"));
        assert_eq!(
            b.rows(0),
            ["Fretless".to_string(), "Slap".into(), "Upright".into()]
        );
        b.turn_sample(0, 9);
        assert_eq!(b.name(0).as_deref(), Some("Upright"));
        b.take_requests();
        b.turn_sample(0, 1);
        assert!(b.take_requests().is_empty(), "no change, no load");
        b.turn_sample(0, -9);
        assert_eq!(b.name(0).as_deref(), Some("Fretless"));
        assert_eq!(b.rows(0)[0], "", "nothing above the first sample");
    }

    #[test]
    fn category_dial_picks_the_first_sample_of_the_next_category() {
        let mut b = lib();
        b.turn_sample(0, 1);
        b.turn_cat(0, 1);
        assert_eq!(b.category(0), Some("Strings"));
        assert_eq!(b.name(0).as_deref(), Some("Cello"));
        b.take_requests();
        b.turn_cat(0, 5);
        assert!(b.take_requests().is_empty(), "already in the last category");
        b.turn_cat(0, -1);
        assert_eq!(b.category(0), Some("Bass"));
    }

    #[test]
    fn newest_request_per_layer_wins() {
        let mut b = lib();
        b.turn_sample(0, 1);
        b.turn_sample(0, 1);
        b.turn_sample(2, 1);
        let r = b.take_requests();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], (0, "Samp Lib/Bass/Slap.nsmp4".to_string()));
        assert_eq!(r[1].0, 2);
        assert!(b.take_requests().is_empty());
    }

    #[test]
    fn observe_loaded_follows_changes_of_the_loaded_name_only() {
        let mut b = lib();
        b.observe_loaded(Some("Nope"));
        assert!(b.entry(0).is_none());
        b.observe_loaded(Some("Cello"));
        assert_eq!(b.category(0), Some("Strings"));
        // The user browses on; the same loaded name is not re-applied every frame.
        b.turn_sample(0, 1);
        b.take_requests();
        let picked = b.name(0);
        b.observe_loaded(Some("Cello"));
        assert_eq!(b.name(0), picked);
        // The app finishes loading the pick: the name changes to the pick's own, which sticks.
        b.observe_loaded(Some("Viola"));
        assert_eq!(b.name(0).as_deref(), Some("Viola"));
        b.observe_loaded(None);
        assert_eq!(b.name(0).as_deref(), Some("Viola"));
        assert!(b.take_requests().is_empty(), "following a load queues none");
    }

    #[test]
    fn empty_catalog_and_new_catalog_drop_stale_picks() {
        let mut b = SampleBrowser::default();
        b.turn_cat(0, 1);
        b.turn_sample(0, 1);
        assert!(b.is_empty() && b.take_requests().is_empty());
        let mut b = lib();
        b.turn_sample(0, 1);
        b.set_catalog(&["Samp Lib/Organ/Pipe.nsmp4".into()]);
        assert!(b.entry(0).is_none());
        assert_eq!(b.cat_count(), 1);
    }
}
