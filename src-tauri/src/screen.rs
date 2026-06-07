use base64::Engine;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

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
            "BattleResultExp" | "BattleResultExpLevelUp" => Self::BattleResultExp,
            "BattleResultBond" | "BattleResultBondLevelUp" => Self::BattleResultBond,
            "BattleResultContinue" => Self::BattleResultContinue,
            "BattleResultFriendRequest" => Self::BattleResultFriendRequest,
            "APRecovery" => Self::APRecovery,
            _ => Self::Unknown,
        };
        Ok(screen)
    }
}

// ---------------------------------------------------------------------------
// Sidecar client — communicates with the Python mash-cv process
// ---------------------------------------------------------------------------

pub struct SidecarClient {
    child: Option<tauri_plugin_shell::process::CommandChild>,
    /// Receives stdout lines forwarded from the async task.
    line_rx: mpsc::Receiver<String>,
    /// Monotonic request id. Every outgoing request carries `{"id": n}` and
    /// the sidecar echoes the same id back; responses whose id does not match
    /// the in-flight request are dropped (they are stale leftovers from a
    /// previously timed-out call). 0 is reserved for "no id sent yet".
    next_id: u64,
    /// Last (width, height) reported by `start_stream`. None until a stream
    /// has been started successfully.
    stream_size: Option<(u32, u32)>,
}

impl SidecarClient {
    /// Spawn the mash-cv sidecar and optionally load templates + config.
    ///
    /// ``server`` is forwarded to the sidecar via a `set_server` REPL call
    /// after templates / config are loaded so the sidecar can pin the
    /// matching OCR model on first use. Defaulting on the sidecar side
    /// means an older sidecar binary that doesn't recognize `set_server`
    /// degrades to JP-only behaviour rather than failing the spawn.
    pub fn spawn(
        app: &tauri::AppHandle,
        templates_dir: Option<&Path>,
        config_path: Option<&Path>,
        server: crate::Server,
    ) -> Result<Self, String> {
        let exe = crate::resolve_sidecar_exe(app)
            .ok_or_else(|| "无法解析 mash-cv runtime，请先安装 CV 运行时".to_string())?;
        if !exe.exists() {
            return Err(format!(
                "未找到 mash-cv runtime 可执行文件 ({}). 请先下载并安装当前版本需要的 CV 运行时。",
                exe.display()
            ));
        }
        let code_dir = crate::resolve_sidecar_code_dir(app)
            .ok_or_else(|| "无法解析 mash-cv code runtime，请先安装 CV 代码包".to_string())?;
        if !code_dir.join("mash_cv").is_dir() {
            return Err(format!(
                "未找到 mash-cv code 包 ({}). 请先下载并安装当前版本需要的 CV 代码包。",
                code_dir.display()
            ));
        }
        let models_dir = crate::resolve_sidecar_models_dir(app)
            .ok_or_else(|| "无法解析 mash-cv OCR 模型目录，请先安装 CV runtime 包".to_string())?;
        if !models_dir.is_dir() {
            return Err(format!(
                "未找到 mash-cv OCR 模型目录 ({}). 请先下载并安装当前版本需要的 CV runtime 包。",
                models_dir.display()
            ));
        }

        let cmd = app
            .shell()
            .command(&exe)
            .env("MASH_CV_CODE_DIR", code_dir.to_string_lossy().to_string())
            .env(
                "MASH_CV_MODELS_DIR",
                models_dir.to_string_lossy().to_string(),
            )
            .env("PYTHONPATH", code_dir.to_string_lossy().to_string());
        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

        // Bridge the async tokio receiver into a sync std::mpsc channel so the
        // blocking runner thread can call recv() without an async runtime.
        let (line_tx, line_rx) = mpsc::channel::<String>();

        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) => {
                        let line = String::from_utf8_lossy(&bytes).trim().to_string();
                        if !line.is_empty() {
                            if line_tx.send(line).is_err() {
                                break;
                            }
                        }
                    }
                    CommandEvent::Stderr(bytes) => {
                        let msg = String::from_utf8_lossy(&bytes);
                        eprintln!("[mash-cv stderr] {msg}");
                    }
                    CommandEvent::Error(e) => {
                        eprintln!("[mash-cv error] {e}");
                    }
                    CommandEvent::Terminated(_) => break,
                    _ => {}
                }
            }
        });

        let mut client = Self {
            child: Some(child),
            line_rx,
            next_id: 1,
            stream_size: None,
        };

        // Wait for the sidecar to finish its cold-boot before sending any real
        // commands. The PyInstaller onefile bundle (OpenCV + PyAV) can take
        // 20-40s on macOS the first time it's extracted. A dedicated `ping`
        // response tells us Python's REPL is running and keeps subsequent
        // request/response pairs in lockstep.
        let ping = serde_json::json!({ "cmd": "ping" });
        match client.send_recv_with_timeout(&ping, Duration::from_secs(90)) {
            Ok(resp) => eprintln!("[mash-cv] ping -> {resp}"),
            Err(e) => return Err(format!("sidecar did not become ready: {e}")),
        }

        if let Some(dir) = templates_dir {
            if dir.exists() {
                let req = serde_json::json!({
                    "cmd": "load_templates",
                    "dir": dir.to_string_lossy(),
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_templates -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_templates failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] templates dir missing: {}", dir.display());
            }
        }

        if let Some(path) = config_path {
            if path.exists() {
                let req = serde_json::json!({
                    "cmd": "load_config",
                    "path": path.to_string_lossy(),
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_config -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_config failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] config path missing: {}", path.display());
            }
        }

        // Tell the sidecar which server's OCR model to use. Sent after
        // templates + config so a fresh `_ocr_engine` rebuild sees the
        // right model on the first `find_supports` call. Failures are
        // logged but non-fatal: an older sidecar binary that doesn't
        // know `set_server` simply stays on its compile-time default.
        let req = serde_json::json!({
            "cmd": "set_server",
            "server": server.to_string(),
        });
        match client.send_recv(&req) {
            Ok(resp) => eprintln!("[mash-cv] set_server -> {resp}"),
            Err(e) => eprintln!("[mash-cv] set_server failed (sidecar may be stale): {e}"),
        }

        Ok(client)
    }

    /// Send a JSON command and wait for the JSON response line.
    fn send_recv(&mut self, request: &serde_json::Value) -> Result<serde_json::Value, String> {
        self.send_recv_with_timeout(request, Duration::from_secs(10))
    }

    /// Like `send_recv` but with a caller-specified timeout. Used by commands
    /// that can legitimately take longer than 10s (sidecar cold boot, scrcpy
    /// handshake).
    ///
    /// Tags the outgoing request with a monotonic ``id`` and discards any
    /// response whose ``id`` doesn't match -- those are stale leftovers from
    /// a previously timed-out call still in flight from the sidecar.
    fn send_recv_with_timeout(
        &mut self,
        request: &serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);

        let mut tagged = request.clone();
        if let Some(obj) = tagged.as_object_mut() {
            obj.insert("id".into(), serde_json::Value::from(id));
        } else {
            return Err(format!(
                "send_recv: request must be a JSON object, got {request}"
            ));
        }

        let mut line = tagged.to_string();
        line.push('\n');
        self.child
            .as_mut()
            .ok_or_else(|| "sidecar process already stopped".to_string())?
            .write(line.as_bytes())
            .map_err(|e| format!("failed to write to sidecar: {e}"))?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| format!("sidecar response timeout (waiting for id={id})"))?;
            let response = self
                .line_rx
                .recv_timeout(remaining)
                .map_err(|e| format!("sidecar response timeout: {e}"))?;
            let parsed: serde_json::Value = serde_json::from_str(&response)
                .map_err(|e| format!("invalid JSON from sidecar: {e}: {response}"))?;
            match parsed.get("id").and_then(|v| v.as_u64()) {
                Some(rid) if rid == id => return Ok(parsed),
                Some(rid) => {
                    eprintln!(
                        "[mash-cv] dropping stale response id={rid} (expected {id}): {response}"
                    );
                    continue;
                }
                None => {
                    // Strict mode: a sidecar built against this Rust client
                    // must echo the id. A response without one almost certainly
                    // means the sidecar binary is from an older build -- tell
                    // the operator instead of silently corrupting CV results.
                    return Err(format!(
                        "sidecar response missing 'id' field (rebuild mash-cv sidecar): {response}"
                    ));
                }
            }
        }
    }

    /// Helper to inject `imagePath` into the request when the caller provided one.
    /// Passing `None` makes the sidecar read the latest frame from the scrcpy stream.
    fn add_image_path(req: &mut serde_json::Value, image_path: Option<&Path>) {
        if let Some(p) = image_path {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "imagePath".into(),
                    serde_json::Value::String(p.to_string_lossy().into_owned()),
                );
            }
        }
    }

    /// Detect which screen is shown. Pass `None` to use the live scrcpy frame.
    pub fn detect(&mut self, image_path: Option<&Path>) -> Result<Screen, String> {
        let (screen, _) = self.detect_full(image_path)?;
        Ok(screen)
    }

    /// Like `detect` but also returns the classifier score.
    pub fn detect_full(&mut self, image_path: Option<&Path>) -> Result<(Screen, f64), String> {
        let (screen_str, score) = self.detect_label_full(image_path)?;
        let screen = screen_str.parse::<Screen>().unwrap_or(Screen::Unknown);
        Ok((screen, score))
    }

    /// Like `detect_full`, but preserves the raw configured screen name.
    pub fn detect_label_full(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<(String, f64), String> {
        let mut req = serde_json::json!({ "cmd": "detect" });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let screen_str = resp["screen"].as_str().unwrap_or("Unknown");
        let score = resp["score"].as_f64().unwrap_or(0.0);
        Ok((screen_str.to_string(), score))
    }

    /// Search for a template element within a region. Pass `None` to use the
    /// live scrcpy frame.
    pub fn find_element(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        // Sidecar surfaces hard errors (e.g. ``template not loaded: <key>``)
        // through the ``error`` field. Treat those as Err so callers don't
        // silently degrade to "not found" — historically this masked a
        // missing-template bug for hours during the support-select work.
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Like `find_element`, but returns the full match (score + bounding box)
    /// for debug/visualization purposes.
    pub fn find_element_full(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Look up an element by `(screen, element)` name in the loaded cv.json
    /// and run template matching for it.
    pub fn find_element_by_name(
        &mut self,
        image_path: Option<&Path>,
        screen: &str,
        element: &str,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element_by_name",
            "screen": screen,
            "element": element,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            if !resp.get("found").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Err(err.to_string());
            }
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Identify the suit and (optionally) servant occupying each fixed
    /// command-card slot on the attack screen.
    ///
    /// ``card_regions`` overrides the sidecar's built-in five-slot layout
    /// — pass ``None`` to use the defaults. ``assets_dir`` must contain
    /// ``{servant_id}/card_servant_*.png`` for identification to succeed;
    /// when missing or empty, only suit + slot position are returned.
    pub fn find_command_cards(
        &mut self,
        image_path: Option<&Path>,
        card_regions: Option<&[NormRect]>,
        servant_ids: &[u32],
        assets_dir: Option<&Path>,
    ) -> Result<Vec<CommandCardMatch>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_command_cards",
            "servantIds": servant_ids,
        });
        if let Some(regions) = card_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "cardRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        if let Some(dir) = assets_dir {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "assetsDir".into(),
                    serde_json::Value::String(dir.to_string_lossy().into_owned()),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            // Sidecar reports a missing frame / bad path here. Surface it.
            return Err(err.to_string());
        }
        let cards = resp
            .get("cards")
            .ok_or_else(|| "find_command_cards: response missing 'cards'".to_string())?;
        serde_json::from_value::<Vec<CommandCardMatch>>(cards.clone())
            .map_err(|e| format!("invalid command-card response: {e}"))
    }

    /// Report whether each fixed Noble Phantasm card slot currently holds
    /// a card. ``np_regions`` overrides the sidecar's built-in three-slot
    /// layout — pass ``None`` to use the defaults.
    pub fn find_noble_phantasms(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_noble_phantasms",
        });
        if let Some(regions) = np_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "npRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        let slots = resp
            .get("slots")
            .ok_or_else(|| "find_noble_phantasms: response missing 'slots'".to_string())?;
        serde_json::from_value::<Vec<NoblePhantasmMatch>>(slots.clone())
            .map_err(|e| format!("invalid noble-phantasm response: {e}"))
    }

    /// OCR the support-select screen's list region and return rows whose
    /// servant-name + NP-name fragments fuzzy-match one of ``expected_names`` and
    /// any of ``expected_np_names`` and sit close enough vertically to be
    /// part of the same row. Defaults (list region, thresholds, pair_dy)
    /// are owned by the sidecar; this binding stays minimal so retuning
    /// happens on the Python side.
    pub fn find_supports(
        &mut self,
        image_path: Option<&Path>,
        expected_name: &str,
        expected_names: &[String],
        expected_np_names: &[String],
        include_support_details: bool,
    ) -> Result<FindSupportsResult, String> {
        let mut req = serde_json::json!({
            "cmd": "find_supports",
            "expectedName": expected_name,
            "expectedNames": expected_names,
            "expectedNpNames": expected_np_names,
            "includeSupportDetails": include_support_details,
        });
        Self::add_image_path(&mut req, image_path);

        // OCR cold-start (loading the ONNX model on first call) can run
        // 5-10s on a fresh sidecar; bump the per-call timeout accordingly.
        // Subsequent calls return in <1s.
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindSupportsResult>(resp)
            .map_err(|e| format!("invalid find_supports response: {e}"))
    }

    /// Run OCR inside the given normalized region and return raw
    /// fragments plus a joined text blob.
    pub fn ocr_region(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<OcrRegionResult, String> {
        let mut req = serde_json::json!({
            "cmd": "ocr_region",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<OcrRegionResult>(resp)
            .map_err(|e| format!("invalid ocr_region response: {e}"))
    }

    /// Read the servant-enhancement level pair (``current/max``) using the
    /// sidecar's digit-template mapper.
    pub fn read_level_digits(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<LevelDigitsResult, String> {
        let mut req = serde_json::json!({
            "cmd": "read_level_digits",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<LevelDigitsResult>(resp)
            .map_err(|e| format!("invalid read_level_digits response: {e}"))
    }

    /// Score a support row's CE icon against the bundled template.
    ///
    /// The runner calls this once per OCR-matched support row when a CE
    /// is pinned on the team-builder support slot. ``region`` is the
    /// search window in absolute normalized coordinates (computed by
    /// applying ``SUPPORT_CE_OFFSET_IN_ROW`` to the row's bbox).
    /// ``template_path`` points at ``assets/ces/{id}/card_ce.png``.
    ///
    /// Returns ``(score, passed)``; both are ``(0.0, false)`` if the
    /// template can't be read or the crop is empty so the caller can
    /// treat read failures the same as score failures.
    pub fn verify_support_ce(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        template_path: &Path,
        threshold: f64,
        options: SupportCeVerificationOptions,
    ) -> Result<SupportCeVerificationResult, String> {
        let mut req = serde_json::json!({
            "cmd": "verify_support_ce",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templatePath": template_path.to_string_lossy(),
            "threshold": threshold,
            "mlbRequired": options.mlb_required,
            "grandBondCeMode": options.grand_bond_ce_mode,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<SupportCeVerificationResult>(resp)
            .map_err(|e| format!("invalid verify_support_ce response: {e}"))
    }

    /// Search for an arbitrary grayscale template file within a region.
    #[allow(dead_code)]
    pub fn find_region(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "threshold": threshold,
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template,
    /// returning the raw match payload even when it misses the threshold.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop_full(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "threshold": threshold,
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    pub fn find_enhancement_servant_grid(
        &mut self,
        image_path: Option<&Path>,
        face_template_paths: &[PathBuf],
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
        retry_seconds: f64,
    ) -> Result<FindEnhancementServantGridResult, String> {
        let mut req = serde_json::json!({
            "cmd": "find_enhancement_servant_grid",
            "anchorTemplateKey": "text_servant_avatar_bottom_line",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "faceThreshold": threshold,
            "retrySeconds": retry_seconds,
            "retryIntervalSeconds": 0.15,
            "faceTemplatePaths": face_template_paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindEnhancementServantGridResult>(resp)
            .map_err(|e| format!("invalid find_enhancement_servant_grid response: {e}"))
    }

    /// Send a `read_battle_scene` request to the sidecar and return the
    /// raw JSON response. Shared by both the lean (`read_battle_scene`)
    /// and diagnostic (`read_battle_scene_debug`) variants so the
    /// request shape stays in one place.
    fn send_read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        debug: bool,
    ) -> Result<serde_json::Value, String> {
        let mut req = serde_json::json!({
            "cmd": "read_battle_scene",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        if debug {
            req["debug"] = serde_json::Value::Bool(true);
        }
        Self::add_image_path(&mut req, image_path);
        self.send_recv(&req)
    }

    /// Read the current battle-scene indicator (`m` of `n`) drawn next to
    /// the BATTLE label in the top-right HUD. Returns `None` when the
    /// sidecar cannot resolve both numbers (anchor missing, NP overlay
    /// covering the strip, etc.).
    pub fn read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<Option<(u32, u32)>, String> {
        let resp = self.send_read_battle_scene(image_path, region, false)?;
        let scene = resp["scene"].as_u64().map(|n| n as u32);
        let total = resp["total"].as_u64().map(|n| n as u32);
        Ok(scene.zip(total))
    }

    /// Diagnostic variant of [`Self::read_battle_scene`] that asks the
    /// sidecar for the full intermediate state (anchor score & box,
    /// strip, every above-threshold digit candidate, the kept set after
    /// NMS, the chosen split + best gap, and a `failReason` enum). Used
    /// only by the `debug_read_battle_scene` Tauri command — the runner
    /// stays on the lean variant.
    pub fn read_battle_scene_debug(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<serde_json::Value, String> {
        self.send_read_battle_scene(image_path, region, true)
    }

    /// Start the scrcpy server on the device and begin streaming.
    /// Returns the negotiated codec (width, height) -- already downscaled by
    /// the device if `max_size` was non-zero, so this is what every decoded
    /// frame will measure, not the raw display size.
    pub fn start_stream(
        &mut self,
        adb_path: &Path,
        jar_path: &Path,
        serial: Option<&str>,
        max_size: u32,
        bit_rate: u32,
    ) -> Result<(u32, u32), String> {
        let mut req = serde_json::json!({
            "cmd": "start_stream",
            "adbPath": adb_path.to_string_lossy(),
            "jarPath": jar_path.to_string_lossy(),
            "maxSize": max_size,
            "bitRate": bit_rate,
        });
        if let Some(s) = serial {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("serial".into(), serde_json::Value::String(s.to_string()));
            }
        }
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(45))?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("start_stream failed: {err}"));
        }
        let w = resp["width"].as_u64().unwrap_or(0) as u32;
        let h = resp["height"].as_u64().unwrap_or(0) as u32;
        if w == 0 || h == 0 {
            return Err("start_stream returned invalid dimensions".into());
        }
        self.stream_size = Some((w, h));
        Ok((w, h))
    }

    /// Stop the scrcpy server / decoder thread. Safe to call if not started.
    pub fn stop_stream(&mut self) -> Result<(), String> {
        let req = serde_json::json!({ "cmd": "stop_stream" });
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("stop_stream failed: {err}"));
        }
        self.stream_size = None;
        Ok(())
    }

    /// Return the (width, height) of the live stream, if one was started.
    pub fn stream_size(&self) -> Option<(u32, u32)> {
        self.stream_size
    }

    /// Return the latest decoded frame as a JPEG byte buffer. ``wait_seconds``
    /// is the maximum time the sidecar will block waiting for a fresh frame
    /// (0 = return immediately if none is cached).
    pub fn get_frame_jpeg(&mut self, wait_seconds: f64) -> Result<Vec<u8>, String> {
        let req = serde_json::json!({
            "cmd": "get_frame",
            "waitSeconds": wait_seconds,
        });
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("get_frame failed: {err}"));
        }
        let b64 = resp["jpegB64"]
            .as_str()
            .ok_or_else(|| "get_frame missing jpegB64".to_string())?;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 in jpegB64: {e}"))
    }

    /// Tell the sidecar to exit.
    pub fn shutdown(&mut self) {
        // Best-effort: ask the device-side scrcpy server to exit cleanly
        // before we kill the host process. Without this, abruptly killing the
        // sidecar mid-stream relies on scrcpy's `cleanup=true` to GC the
        // server -- which works most of the time but occasionally leaves a
        // zombie `app_process` on the device.
        if self.child.is_some() && self.stream_size.is_some() {
            if let Err(e) = self.stop_stream() {
                eprintln!("[mash-cv] stop_stream during shutdown failed: {e}");
            }
        }
        if let Some(mut child) = self.child.take() {
            let req = serde_json::json!({"cmd": "quit"});
            let mut line = req.to_string();
            line.push('\n');
            let _ = child.write(line.as_bytes());
            std::thread::sleep(Duration::from_millis(150));
            let _ = child.kill();
        }
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_image_path_inserts_field_when_some() {
        let mut req = serde_json::json!({"cmd": "detect"});
        SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/x.png")));
        assert_eq!(req["cmd"], serde_json::json!("detect"));
        assert_eq!(req["imagePath"], serde_json::json!("/tmp/x.png"));
    }

    #[test]
    fn add_image_path_is_a_noop_when_none() {
        let mut req = serde_json::json!({"cmd": "detect"});
        SidecarClient::add_image_path(&mut req, None);
        // No `imagePath` key => sidecar falls back to the live frame.
        assert!(
            req.as_object().unwrap().get("imagePath").is_none(),
            "expected no imagePath key, got {req:?}"
        );
    }

    #[test]
    fn add_image_path_preserves_existing_fields() {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": "btn_ok",
            "threshold": 0.8,
        });
        SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/scene.png")));
        assert_eq!(req["templateKey"], serde_json::json!("btn_ok"));
        assert_eq!(req["threshold"], serde_json::json!(0.8));
        assert_eq!(req["imagePath"], serde_json::json!("/tmp/scene.png"));
    }

    #[test]
    fn add_image_path_does_nothing_when_request_is_not_an_object() {
        // The early-return on `as_object_mut` keeps the helper safe to
        // call against arbitrary `serde_json::Value` payloads.
        let mut req = serde_json::json!([1, 2, 3]);
        SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/x.png")));
        assert_eq!(req, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn ap_recovery_screen_round_trips_display_name() {
        assert_eq!(Screen::APRecovery.to_string(), "APRecovery");
        assert_eq!("APRecovery".parse::<Screen>().unwrap(), Screen::APRecovery);
    }

    #[test]
    fn bond_level_up_screen_routes_to_bond_handler() {
        assert_eq!(
            "BattleResultBondLevelUp".parse::<Screen>().unwrap(),
            Screen::BattleResultBond
        );
    }

    #[test]
    fn exp_level_up_screen_routes_to_exp_handler() {
        assert_eq!(
            "BattleResultExpLevelUp".parse::<Screen>().unwrap(),
            Screen::BattleResultExp
        );
    }

    #[test]
    fn loot_event_screen_routes_to_loot_handler() {
        assert_eq!(
            "BattleResultLootEvent".parse::<Screen>().unwrap(),
            Screen::BattleResultLoot
        );
    }
}
