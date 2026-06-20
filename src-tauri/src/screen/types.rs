use std::str::FromStr;

// ---------------------------------------------------------------------------
// Core geometry types (normalized 0.0..1.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn to_physical(&self, width: u32, height: u32) -> (u32, u32) {
        (
            (self.x * width as f64) as u32,
            (self.y * height as f64) as u32,
        )
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SupportCeVerificationOptions {
    #[serde(default)]
    pub mlb_required: bool,
    #[serde(default)]
    pub grand_bond_ce_mode: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportCeIconCheck {
    pub kind: String,
    pub template_key: String,
    pub region: NormRect,
    pub score: f64,
    pub passed: bool,
    pub threshold: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportCeArtworkCheck {
    pub variant: String,
    pub region_kind: String,
    pub score: f64,
    pub threshold: f64,
    pub passed: bool,
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportCeVerificationResult {
    pub score: f64,
    pub passed: bool,
    /// Threshold the sidecar actually compared `score` against. Equals
    /// the runner's `SUPPORT_CE_THRESHOLD` for normal rows, but is
    /// relaxed (currently to 0.65) for Grand-Saber bond / bondNp slots
    /// because bond-CE artwork matches sit closer to the threshold —
    /// see `BOND_CE_ARTWORK_THRESHOLD` in `sidecar/mash_cv/mash_cv/cv.py`.
    /// Surfacing this lets the runner log and the debug overlay display
    /// the threshold that was actually applied instead of the static
    /// `SUPPORT_CE_THRESHOLD`, which would otherwise contradict the
    /// `passed` verdict for bond rows scoring 0.65–0.70.
    #[serde(default)]
    pub threshold: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub icon_checks: Vec<SupportCeIconCheck>,
    #[serde(default)]
    pub artwork_checks: Vec<SupportCeArtworkCheck>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementMatch {
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    pub region: Option<NormRect>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantGridAnchor {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub score: f64,
    #[serde(default)]
    pub edge_score: f64,
    #[serde(default)]
    pub gray_score: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub row: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantGridCell {
    pub row: u32,
    pub col: u32,
    pub region: NormRect,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantGridFaceMatch {
    pub template: String,
    pub template_path: String,
    #[serde(default)]
    pub row: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
    pub found: bool,
    pub score: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub region: Option<NormRect>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantGridDiagnostics {
    #[serde(default)]
    pub fail_reason: Option<String>,
    #[serde(default)]
    pub anchor_template_key: String,
    #[serde(default)]
    pub anchor_edge_threshold: f64,
    #[serde(default)]
    pub anchor_gray_threshold: f64,
    #[serde(default)]
    pub face_threshold: f64,
    #[serde(default)]
    pub region: Option<NormRect>,
    #[serde(default)]
    pub anchor_count: u32,
    #[serde(default)]
    pub grid_cell_count: u32,
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindEnhancementServantGridResult {
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    #[serde(default)]
    pub best: Option<ServantGridFaceMatch>,
    #[serde(default)]
    pub anchors: Vec<ServantGridAnchor>,
    #[serde(default)]
    pub reference_anchor: Option<ServantGridAnchor>,
    #[serde(default)]
    pub grid_cells: Vec<ServantGridCell>,
    #[serde(default)]
    pub matches: Vec<ServantGridFaceMatch>,
    pub diagnostics: ServantGridDiagnostics,
}

/// Per-digit recognition signal for one crit-percentage slot. Surfaced
/// for the debug UI so an empty / sub-threshold / valid-but-discarded
/// read can be told apart from a genuinely missing crit value.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CritDigitRead {
    /// Best-scoring digit template (0..9) for this slot, or ``None``
    /// when the slot ROI was empty or no template fit at any scale.
    #[serde(default)]
    pub digit: Option<u32>,
    /// Raw TM_CCOEFF_NORMED score of ``digit`` regardless of threshold.
    pub score: f64,
    /// Whether ``digit`` cleared the per-slot acceptance threshold and
    /// contributed to the assembled crit value.
    pub kept: bool,
}

/// One detected command-card slot on the attack screen.
///
/// The five slot bboxes are fixed positions (configured in the Python
/// sidecar's ``DEFAULT_COMMAND_CARD_SLOTS``) and always present in the
/// response. ``suit`` / ``icon_*`` are filled in when at least one suit
/// icon template scores inside the slot. ``servant_id`` / ``ascension``
/// / ``face_score`` are populated only when the caller passes a non-empty
/// candidate list **and** an assets directory containing
/// ``{id}/card_servant_*.png`` files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCardMatch {
    pub slot: u32,
    /// Tap point (slot center) in normalized coordinates.
    pub x: f64,
    pub y: f64,
    /// The slot bbox itself.
    pub card_region: NormRect,
    /// Upper-portion of the slot used as the face-template search area.
    pub face_region: NormRect,
    /// Per-digit ROIs (hundreds, tens, ones) where the slot-relative crit
    /// percentage is OCR'd. Each digit is matched inside its own tight
    /// region rather than across a single wide strip — see
    /// ``COMMAND_CARD_CRIT_DIGIT_REGIONS`` in the sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_digit_regions: Option<Vec<NormRect>>,
    /// Per-slot recognition signal for the crit digits. Surfaced even
    /// when ``crit_chance`` is ``None`` so the debug UI can show why
    /// (which slot fell below threshold or assembled to an invalid
    /// non-multiple-of-10 value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_digit_reads: Option<Vec<CritDigitRead>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Suit code: ``"a"`` (Arts), ``"b"`` (Buster), or ``"q"`` (Quick).
    pub suit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_region: Option<NormRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servant_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascension: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_chance: Option<u32>,
}

/// Result of NP-readiness detection for a single Noble Phantasm card slot.
///
/// One record is returned per slot regardless of readiness so callers can
/// render every slot in a debug overlay. ``ready`` is the primary signal;
/// ``edge_frac`` and ``std_bgr`` expose the underlying measurements so
/// thresholds can be re-tuned from the debug UI without code changes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoblePhantasmMatch {
    pub slot: u32,
    pub card_region: NormRect,
    pub ready: bool,
    pub edge_frac: f64,
    pub std_bgr: f64,
    /// Edge-fraction threshold the sidecar used to flag this slot. The
    /// detector now adapts per frame (see ``_decide_np_ready`` in
    /// ``cv.py``) so the value can change between calls — surfacing it
    /// here lets the debug overlay explain why a given slot landed on
    /// either side of the line. Optional for backward compat with older
    /// sidecar bundles that don't emit the field.
    #[serde(default)]
    pub edge_threshold: f64,
}

/// One support row whose servant-name and NP-name fragments OCR'd, fuzzy-
/// matched the expected strings, and were paired by vertical proximity.
///
/// Returned by ``SidecarClient::find_supports`` for the support-select
/// screen. The list is scrollable so row positions are dynamic — the
/// detector OCRs the whole list region and synthesizes ``row_region`` from
/// the union of the matched name + NP fragment bboxes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportRowMatch {
    pub row_region: NormRect,
    pub tap: Point,
    pub name_text: String,
    pub name_score: f64,
    #[serde(default)]
    pub name_matched_name: Option<String>,
    pub name_region: NormRect,
    pub np_text: String,
    pub np_score: f64,
    pub np_region: NormRect,
    /// Optional right-side support-row anchor bbox. In Grand support mode
    /// this is the "助战编队确认" panel; runners use it for row-relative
    /// regions whose vertical placement is more stable than OCR text bboxes.
    #[serde(default)]
    pub score_anchor: Option<NormRect>,
    /// Which entry of the caller's ``expected_np_names`` list won the fuzzy
    /// match — useful when a servant has multiple candidate NPs.
    pub np_matched_name: String,
    /// Parsed NP level shown at the end of the row. Present only when the
    /// caller enables CN support-detail extraction.
    #[serde(default)]
    pub np_level: Option<u32>,
    /// Current right-side skill panel kind: "owned" or "append".
    #[serde(default)]
    pub skill_panel: Option<String>,
    /// Visible owned skill levels. Missing/unopened levels are `None`.
    #[serde(default)]
    pub skill_levels: Vec<Option<u32>>,
    /// Visible append skill levels. Missing/unopened levels are `None`.
    #[serde(default)]
    pub append_skill_levels: Vec<Option<u32>>,
    /// Per-slot skill recognition diagnostics returned by the sidecar.
    #[serde(default)]
    pub skill_level_diagnostics: Vec<serde_json::Value>,
}

/// One OCR fragment that fuzzy-matched the expected servant-name or NP-name
/// above its threshold. Surfaced through diagnostics so the debug UI can
/// render misses (a candidate that matched but had no proximity partner).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportCandidate {
    pub text: String,
    pub score: f64,
    pub region: NormRect,
    /// Identifies which expected name / NP candidate won the fuzzy match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_name: Option<String>,
}

/// One raw OCR fragment from the support-list region, regardless of whether
/// it scored above the name / NP fuzzy thresholds. Surfaced so the debug UI
/// can show *why* a match failed — typically the closest NP fragment scored
/// 0.4-0.5 against the expected text, just under the 0.65 threshold,
/// because the mooncell `name_cn` doesn't match the in-game CN string.
/// Without this, the only feedback for a 0-row response was "OCR found N
/// fragments" without any way to see what those fragments said.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportFragment {
    pub text: String,
    pub region: NormRect,
    /// RapidOCR's own per-fragment confidence (independent of our fuzzy
    /// matching). 0.0 when the OCR backend doesn't return a confidence.
    #[serde(default)]
    pub ocr_confidence: f64,
    /// Fuzzy score against the expected servant name (0.0-1.0).
    #[serde(default)]
    pub name_score: f64,
    /// Best fuzzy score across the expected NP list (0.0-1.0); 0.0 when
    /// no NPs were expected.
    #[serde(default)]
    pub best_np_score: f64,
    /// Which expected NP produced ``best_np_score``; empty string when
    /// no NPs were expected.
    #[serde(default)]
    pub best_np_name: String,
}

/// One raw OCR fragment returned by the generic `ocr_region` sidecar
/// command. Used by the enhancement runner for OCR-driven page routing
/// and text/button lookup outside the battle flow.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrFragment {
    pub text: String,
    pub region: NormRect,
    #[serde(default)]
    pub ocr_confidence: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRegionResult {
    #[serde(default)]
    pub fragments: Vec<OcrFragment>,
    #[serde(default)]
    pub full_text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelDigitsResult {
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub current: Option<u32>,
    #[serde(default, rename = "max")]
    pub max_level: Option<u32>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub fail_reason: Option<String>,
}

/// Diagnostic payload accompanying every ``find_supports`` response. Always
/// returned (even when ``supports`` is empty) so the debug UI can show
/// "OCR ran but matched nothing" vs. "OCR didn't find any candidates".
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportDiagnostics {
    pub list_region: NormRect,
    pub name_candidates: Vec<SupportCandidate>,
    pub np_candidates: Vec<SupportCandidate>,
    pub fragment_count: u32,
    /// Every OCR fragment (above and below threshold). New in the
    /// fragment-surfacing patch; `#[serde(default)]` keeps the Rust
    /// client compatible with an older sidecar that doesn't emit it.
    #[serde(default)]
    pub fragments: Vec<SupportFragment>,
    /// True when the sidecar synthesized rows from name candidates alone
    /// because the strict pairing path couldn't run (no NPs expected, or
    /// none cleared the NP threshold). Defaults to false for backwards
    /// compatibility with older sidecars.
    #[serde(default)]
    pub name_only_fallback: bool,
    /// Discriminator for `name_only_fallback`: empty string (default)
    /// means strict pairing; "noNpExpected" means CN translation
    /// dropped every NP for this servant; "noNpAboveThreshold" means
    /// at least one NP was expected but none of the OCR fragments
    /// scored high enough — almost always a sign the mooncell CN
    /// translation doesn't match the in-game text.
    #[serde(default)]
    pub name_only_reason: String,
    #[serde(default)]
    pub cv_file: String,
    #[serde(default)]
    pub cv_fingerprint: String,
    #[serde(default)]
    pub support_skill_contour_split: bool,
    #[serde(default)]
    pub support_row_anchor_search_region: Option<NormRect>,
    /// Every "助战编队确认" button bbox currently visible on the support
    /// list, top-to-bottom. Used by the runner to size its scroll swipe
    /// so the lowest visible button lands near the top of the next view
    /// (avoids the legacy fixed-distance swipe overshooting and pushing
    /// the bottom row off-screen).
    #[serde(default)]
    pub confirm_button_anchors: Vec<NormRect>,
    /// Whether at least one Grand servant ("冠位从者") row is currently
    /// visible. The sidecar probes for the gold-on-blue ribbon at a
    /// fixed offset next to each `confirm_button_anchors` entry rather
    /// than scanning the whole avatar column, which used to false-match
    /// other gold-on-blue chrome and either kept the runner scrolling
    /// past an exhausted Grand section or stopped scrolling too early
    /// on a still-full one. `None` means the active server's template
    /// bundle doesn't ship the ribbon (e.g. JP), so callers should fall
    /// back to the scroll-bar end indicator.
    #[serde(default)]
    pub is_grand_section_visible: Option<bool>,
    /// Per-anchor TM_CCOEFF_NORMED scores for the "冠位从者" ribbon
    /// probe, aligned 1-1 with `confirm_button_anchors`. Each entry
    /// is the max score within that row's badge ROI, or `None` when
    /// the ROI clipped past the frame edge / the template is
    /// unavailable. The debug overlay colours each row's box based
    /// on whether its score cleared the sidecar's match threshold
    /// (currently 0.65) — the aggregate `is_grand_section_visible`
    /// just collapses these to an "any hit" bool and loses the
    /// per-row breakdown the operator needs to spot a non-Grand row
    /// drawn green by a single global flag.
    #[serde(default)]
    pub grand_ribbon_anchor_scores: Vec<Option<f64>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindSupportsResult {
    pub supports: Vec<SupportRowMatch>,
    pub diagnostics: SupportDiagnostics,
}

// ---------------------------------------------------------------------------
// Screen enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Screen {
    TeamConfirm,
    TeamChange,
    SupportSelect,
    ServantSelect,
    Battle,
    Attack,
    /// Battle-result drops/loot summary (1st post-battle page).
    BattleResultLoot,
    /// Master/servant EXP gain summary.
    BattleResultExp,
    /// Bond-points summary.
    BattleResultBond,
    /// Final "Continue / Next" page closing out the result sequence.
    BattleResultContinue,
    /// Friend-request prompt that appears after a battle when an
    /// unfriended support was used.
    BattleResultFriendRequest,
    /// AP recovery dialog shown after tapping repeat when AP is insufficient.
    APRecovery,
    Unknown,
}

impl std::fmt::Display for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TeamConfirm => write!(f, "TeamConfirm"),
            Self::TeamChange => write!(f, "TeamChange"),
            Self::SupportSelect => write!(f, "SupportSelect"),
            Self::ServantSelect => write!(f, "ServantSelect"),
            Self::Battle => write!(f, "Battle"),
            Self::Attack => write!(f, "Attack"),
            Self::BattleResultLoot => write!(f, "BattleResultLoot"),
            Self::BattleResultExp => write!(f, "BattleResultExp"),
            Self::BattleResultBond => write!(f, "BattleResultBond"),
            Self::BattleResultContinue => write!(f, "BattleResultContinue"),
            Self::BattleResultFriendRequest => write!(f, "BattleResultFriendRequest"),
            Self::APRecovery => write!(f, "APRecovery"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl FromStr for Screen {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let screen = match s {
            "TeamConfirm" => Self::TeamConfirm,
            "TeamChange" => Self::TeamChange,
            "SupportSelect" => Self::SupportSelect,
            "ServantSelect" => Self::ServantSelect,
            "Battle" => Self::Battle,
            "Attack" => Self::Attack,
            "BattleResultLoot" | "BattleResultLootEvent" => Self::BattleResultLoot,
            "BattleResultExp" | "BattleResultExpLevelUp" | "BattleResultMasterLevelUp" => {
                Self::BattleResultExp
            }
            "BattleResultBond" | "BattleResultBondLevelUp" => Self::BattleResultBond,
            "BattleResultContinue" => Self::BattleResultContinue,
            "BattleResultFriendRequest" => Self::BattleResultFriendRequest,
            "APRecovery" => Self::APRecovery,
            _ => Self::Unknown,
        };
        Ok(screen)
    }
}
