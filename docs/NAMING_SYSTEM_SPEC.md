# Professional Naming System — Specification

## 1. Overview

Add a flexible, template-driven naming system for video split and merge outputs. This is a new subsystem that sits between the existing split/merge engines and the file system, replacing hardcoded filename generation with a user-configurable pattern engine.

**Scope:** Split output naming + Merge output naming + Subtitle naming.
**Constraint:** Zero breaking changes. All existing behavior must be preservable as a default.

---

## 2. Goals

- Let users define output filename patterns via template strings
- Support common naming modes (sequential, prefix, suffix, chapter, timestamp, etc.)
- Provide live preview as users type their template
- Validate templates for illegal characters, Windows reserved names, length
- Auto-apply naming pattern to subtitle files (SRT) sharing the same base name
- Be usable by beginners (simple defaults) and power users (full template syntax)
- Maintain full backward compatibility — existing workflows must not break

---

## 3. Template Variables

### 3.1 Supported Variables

### 3.1 Supported Variables — Core

| Variable | Description | Example Output |
|----------|-------------|----------------|
| `{filename}` | Original input file stem (no path, no extension) | `Course_Video` |
| `{ext}` | Output file extension (without dot) | `mp4` |
| `{num}` | Sequential index (no padding) | `1`, `2`, `3` |
| `{num2}` | 2-digit zero-padded index | `01`, `02`, `03` |
| `{num3}` | 3-digit zero-padded index | `001`, `002`, `003` |
| `{num4}` | 4-digit zero-padded index | `0001`, `0002`, `0003` |
| `{date}` | ISO date (YYYY-MM-DD) | `2026-05-30` |
| `{time}` | Local time (HH-MM-SS) | `14-30-00` |
| `{start}` | Segment start time (HH-MM-SS) | `00-00-00` |
| `{end}` | Segment end time (HH-MM-SS) | `00-30-00` |
| `{duration}` | Segment duration in seconds | `1800.5` |
| `{chapter}` | Chapter/label name (sanitized) | `Introduction` |
| `{part_label}` | Mode-specific label | `Part`, `Day`, `Week` |
| `{resolution}` | Video resolution | `1920x1080` |
| `{height}` | Video height in pixels | `1080` |
| `{width}` | Video width in pixels | `1920` |

### 3.2 Supported Variables — Extended

| Variable | Description | Example Output |
|----------|-------------|----------------|
| `{folder}` | Parent folder name of input file | `Python_Course` |
| `{playlist}` | Playlist/group name | `Day_01` |
| `{playlist_index}` | Position within playlist group | `1`, `2`, `3` |
| `{original_num}` | Detected leading number in original filename | `01`, `02`, `03` |
| `{video_count}` | Total number of videos in merge | `51` |
| `{total_duration}` | Total duration formatted (e.g., `04h58m`) | `04h58m` |

### 3.3 Subtitle-Specific Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `{lang}` | Language code | `en`, `es`, `ja` |
| `{lang_name}` | Full language name | `English`, `Spanish` |

| Variable | Description | Example |
|----------|-------------|---------|
| `{lang}` | Language code | `en`, `es`, `ja` |
| `{lang_name}` | Full language name | `English`, `Spanish` |

---

## 3.4 Variable Detection: `{original_num}`

The `{original_num}` variable extracts a leading numeric prefix from the input filename if present.

**Algorithm:**
1. Strip path and extension from input filename
2. Match regex `^(\d+)[_\s.-]*(.*)$` against the stem
3. If matched: `{original_num}` = captured digits, `{filename}` = remainder (stripped of leading separator)
4. If no match: `{original_num}` = empty string, `{filename}` = full stem

**Examples:**

| Input Filename | `{original_num}` | `{filename}` |
|----------------|-------------------|--------------|
| `01 Intro.mp4` | `01` | `Intro` |
| `02_Section_A.mp4` | `02` | `Section_A` |
| `lecture-03.mp4` | `03` | `lecture` |
| `Python Course.mp4` | `` | `Python Course` |
| `007_James.mp4` | `007` | `James` |

This allows course creators to preserve original lesson numbers while reformatting.

---

## 3.5 Variable Detection: `{total_duration}`

Format: `{total_duration}` renders as a human-readable total duration string.

**Format rules:**
- If ≥ 1 hour: `{total_duration}` = `{HH}h{MM}m` (e.g., `04h58m`)
- If < 1 hour: `{total_duration}` = `{MM}m{SS}s` (e.g., `45m30s`)
- If < 1 minute: `{total_duration}` = `{SS}s` (e.g., `30s`)

---

## 3.6 Variable Detection: `{folder}`

The `{folder}` variable extracts the name of the parent directory of the input file.

**Rules:**
- Strip all path separators from the parent directory name
- Sanitize: replace spaces with `_` (consistent with template convention)
- If input is a single filename with no directory, `{folder}` = empty string

**Examples:**

| Input Path | `{folder}` |
|------------|------------|
| `C:\Videos\Course\lecture.mp4` | `Course` |
| `./Python Course/01 intro.mp4` | `Python_Course` |
| `video.mp4` | `` |

---

## 3.7 Variable Detection: `{playlist}` and `{playlist_index}`

These variables are used when splitting by playlist groupings.

| Variable | Description |
|----------|-------------|
| `{playlist}` | Name of the playlist group (e.g., `Day_01`, `Week_02`) |
| `{playlist_index}` | 1-based index within that playlist group |

## 4. Naming Modes

### 4.1 Mode List

| Mode | Description | Default Template |
|------|-------------|------------------|
| `sequential` | Basic numbered parts | `{filename}_Part_{num3}` |
| `prefix` | User-defined prefix + number | `{prefix}_{num3}` |
| `suffix` | Filename + user-defined suffix + number | `{filename}_{suffix}_{num3}` |
| `custom` | Full template string | User-defined |
| `timestamp` | Include time ranges | `{filename}_{start}_{end}` |
| `chapter` | Chapter-based names | `{chapter}_{num2}` |
| `playlist` | Group-based names | `{filename}_{playlist}_{num2}` |
| `date` | Include date stamp | `{filename}_{date}_{num2}` |
| `smart_course` | Module/chapter pattern | `Module_{num2}_{chapter}` |

### 4.2 Mode Selection UI

Each mode shows:
- A concise label (e.g., "Sequential", "Chapter", "Timestamp")
- Its default template
- A brief description
- An illustrative before/after example

---

## 5. Data Model

### 5.1 TypeScript Interfaces (Frontend)

```typescript
// Template variable types
type TemplateVariable =
  | 'filename' | 'ext' | 'num' | 'num2' | 'num3' | 'num4'
  | 'date' | 'time' | 'start' | 'end' | 'duration'
  | 'chapter' | 'part_label' | 'resolution' | 'height' | 'width'
  | 'folder' | 'playlist' | 'playlist_index' | 'original_num'
  | 'video_count' | 'total_duration'
  | 'lang' | 'lang_name';

interface NamingMode {
  id: NamingModeId;
  label: string;
  description: string;
  defaultTemplate: string;
  example: { input: string; output: string };
  supportedVariables: TemplateVariable[];
}

type NamingModeId =
  | 'sequential' | 'prefix' | 'suffix' | 'custom'
  | 'timestamp' | 'chapter' | 'playlist' | 'date' | 'smart_course';

// The main configuration object
interface NamingConfig {
  mode: NamingModeId;
  template: string;                        // e.g. "{filename}_Part_{num3}"
  prefix?: string;                         // For prefix mode
  suffix?: string;                         // For suffix mode
  zeroPadding?: 2 | 3 | 4;               // Default: 3
  separator?: string;                      // Default: "_"
}

// Resolved template for a single output
interface ResolvedName {
  baseName: string;   // Without extension
  extension: string;  // Extension with dot
  fullPath: string;   // Complete resolved path
}

// Validation result
interface NamingValidation {
  valid: boolean;
  errors: NamingError[];
  warnings: NamingWarning[];
  resolvedPreview?: string;
  batchPreview?: string[];  // First 5 resolved names for live preview
  totalCount?: number;      // Total number of outputs (for "... N more" display)
}

interface NamingError {
  code: string;
  message: string;
  position?: number;  // Character index in template
}

interface NamingWarning {
  code: string;
  message: string;
}
```

### 5.2 Rust Types (Backend)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingConfig {
    pub mode: NamingModeId,
    pub template: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub zero_padding: Option<u8>,
    pub separator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NamingModeId {
    Sequential,
    Prefix,
    Suffix,
    Custom,
    Timestamp,
    Chapter,
    Playlist,
    Date,
    SmartCourse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingValidation {
    pub valid: bool,
    pub errors: Vec<NamingError>,
    pub warnings: Vec<NamingWarning>,
    pub resolved_preview: Option<String>,
    pub batch_preview: Option<Vec<String>>,
    pub total_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingError {
    pub code: String,
    pub message: String,
    pub position: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingWarning {
    pub code: String,
    pub message: String,
}
```

### 5.3 Where NamingConfig Lives

- **Split screen:** Add `namingConfig: NamingConfig` to `SplitParams`
- **Merge screen:** Add `namingConfig: NamingConfig` to `MergeRequest.splitConfig`
- **Persistence:** Store last-used naming config in the Zustand stores, not in localStorage separately (keep it co-located with other split/merge settings)

---

## 6. Template Parser

### 6.1 Parser Architecture

The parser operates in two phases:

**Phase 1 — Tokenization**
Input: `"{filename}_Part_{num3}.{ext}"`
Output: Tokens:
```
VAR("filename")        // {filename}
LITERAL("_Part_")      // _Part_
VAR("num3")            // {num3}
LITERAL(".")           // .
VAR("ext")             // {ext}
```

**Phase 2 — Resolution**
Given a context (input file info, segment index, time range, etc.), resolve each variable to its string value and concatenate.

### 6.2 Context Object (Passed at Resolution Time)

```typescript
interface TemplateContext {
  // From input file
  filename: string;        // stem only
  extension: string;       // without dot
  folder?: string;         // parent folder name (sanitized)
  originalNum?: string;     // detected leading number in filename

  // From segment/merge item
  index: number;           // 0-based
  startTime: number;       // seconds
  endTime: number;         // seconds
  duration: number;        // seconds

  // From metadata
  chapter?: string;        // label for this segment
  resolution?: string;     // e.g. "1920x1080"
  width?: number;
  height?: number;
  playlist?: string;        // playlist/group name
  playlistIndex?: number;  // 1-based index within playlist group

  // Merge-specific
  videoCount?: number;      // total videos in merge
  totalDuration?: number;   // total duration in seconds

  // Date/time at generation
  date: string;            // YYYY-MM-DD
  time: string;            // HH-MM-SS

  // User inputs for prefix/suffix modes
  prefix?: string;
  suffix?: string;

  // Subtitle-specific
  lang?: string;
  langName?: string;
}
```

### 6.3 Default Templates Per Mode

| Mode | Default Template |
|------|------------------|
| Sequential | `{filename}_Part_{num3}` |
| Prefix | `{prefix}_{num3}` |
| Suffix | `{filename}_{suffix}_{num3}` |
| Custom | `{filename}_Part_{num3}` (user-defined) |
| Timestamp | `{filename}_{start}_{end}` |
| Chapter | `{chapter}_{num2}` |
| Playlist | `{filename}_{playlist}_{num2}` |
| Date | `{filename}_{date}_{num2}` |
| Smart Course | `Module_{num2}_{chapter}` |

### 6.4 Format Helpers (used by context builder)

```typescript
function formatTime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  return `${pad(h)}-${pad(m)}-${pad(s)}`;
}

function pad(n: number, width: number = 2): string {
  return String(n).padStart(width, '0');
}

function formatDate(date: Date): string {
  return date.toISOString().split('T')[0]; // YYYY-MM-DD
}

function formatTimeLocal(date: Date): string {
  const h = date.getHours(), m = date.getMinutes(), s = date.getSeconds();
  return `${pad(h)}-${pad(m)}-${pad(s)}`;
}
```

---

## 7. Validation Rules

### 7.1 Template Validation (Per Keystroke / On Blur)

| Rule | Error Code | Message |
|------|------------|---------|
| Empty template | `EMPTY_TEMPLATE` | Template cannot be empty |
| No variables used | `NO_VARIABLES` | Template should contain at least one variable like `{num}` |
| Unknown variable | `UNKNOWN_VARIABLE` | Unknown variable `{xyz}`. Valid: {filename}, {num}, {num2}, ... |
| Unclosed brace | `UNCLOSED_BRACE` | Unclosed `{` in template |
| Invalid char in literal | `INVALID_CHARS` | Character `X` is not allowed in filenames |

### 7.2 Resolved Filename Validation (When Previewing)

| Rule | Error Code | Message |
|------|------------|---------|
| Windows reserved name | `RESERVED_NAME` | `{name}` is a reserved Windows filename |
| Illegal Windows chars | `ILLEGAL_CHARS` | Filename contains illegal characters |
| Path too long (>200) | `PATH_TOO_LONG` | Filename would be {N} characters (max: 200) |
| Empty after sanitization | `EMPTY_AFTER_SANITIZE` | Filename would be empty after cleaning |

### 7.3 Reserved Windows Names

`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`
(Any stem matching these exactly, or with trailing dot/space, is rejected)

### 7.4 Illegal Characters (Stripped or Rejected)

```
< > : " / \ | ? * and control characters (ASCII < 32)
```

Note: The validation should warn about these rather than auto-strip, because auto-strip can silently change the user's intended name. The preview should show the resolved (post-sanitization) name alongside the template.

### 7.5 Collision & Overwrite Detection

**Collision detection runs before job start, not at template-edit time** (since the full segment list may not be known until preview is generated).

#### 7.5.1 Duplicate Name Detection

| Check | When | Behavior |
|-------|------|----------|
| Same filename twice | Pre-preview | Show warning: "Template produces duplicate filename `{name}` for segments {i} and {j}" |
| No numbering variable | Pre-preview | Show warning: "Template has no sequence variable — all outputs will overwrite each other" |
| Same filename + extension | Pre-job | Block with error: "Duplicate output filename: `{name}`. Cannot proceed." |

#### 7.5.2 Overwrite Detection

| Check | When | Behavior |
|-------|------|----------|
| Output file exists | Pre-job | Show modal: "These files already exist and will be overwritten: {list}. Proceed?" |
| Concurrent job conflict | Pre-job | Same modal, but with both jobs' filenames listed |

#### 7.5.3 Batch Preview (Live, First N Outputs)

Instead of showing only one preview filename, show the **first 5 resolved outputs**:

```
Preview:
  Course_Part_001.mp4
  Course_Part_002.mp4
  Course_Part_003.mp4
  Course_Part_004.mp4
  Course_Part_005.mp4
  ... (3 more)
```

This catches naming mistakes instantly (e.g., missing `{num}` in template).

The backend `validate_naming_template` command accepts `count: usize` and returns a `Vec<String>` of the first N resolved names.

#### 7.5.4 Concurrent Job Isolation

Each job gets a unique `jobId`. Output paths are resolved relative to `outputDir + jobId/` during execution, or the jobId is embedded in temp filenames. This prevents Job A and Job B (with the same naming template) from ever writing to the same file.

---

## 8. UI Design

### 8.1 Split Screen — Naming Section

```
┌─────────────────────────────────────────────────────────────────┐
│ OUTPUT NAMING                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [Sequential] [Prefix] [Suffix] [Custom] [Timestamp] [Chapter] │
│  [Playlist] [Date] [Smart Course]                              │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ {filename}_Part_{num3}                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  Preview:                                                       │
│  ├─ Course_Part_001.mp4                                        │
│  ├─ Course_Part_002.mp4                                        │
│  ├─ Course_Part_003.mp4                                        │
│  ├─ Course_Part_004.mp4                                        │
│  └─ ... (96 more parts)                                       │
│                                                                 │
│  ⚠ Template has no sequence variable — all outputs identical   │
│                                                                 │
│  Advanced Options (collapsible):                               │
│  ├─ Zero Padding: [2] [3] [4]                                  │
│  ├─ Separator: [_]                                             │
│  └─ Variables cheat sheet: {filename} {num3} {start} {end}... │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 8.2 Merge Screen — Naming Section

Appears in the Output Settings area:

```
┌─────────────────────────────────────────────────────────────────┐
│ OUTPUT NAMING                                       [?] Help    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [Sequential] [Prefix] [Custom] [Date]                          │
│                                                                 │
│  Template: {filename}_Merged_{num3}                             │
│  Preview: Course_Merged_001.mp4                                  │
│                                                                 │
│  ☑ Apply naming to split output (if split enabled)             │
│  ☑ Apply naming to subtitle files                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 8.3 Component Inventory

| Component | Description |
|-----------|-------------|
| `NamingModeSelector` | Pill/radio group to pick naming mode |
| `NamingTemplateInput` | Text input with syntax highlighting for variables |
| `NamingLivePreview` | Shows resolved filename in real-time |
| `NamingAdvancedOptions` | Collapsible section with padding, separator, variable reference |
| `NamingValidationFeedback` | Inline error/warning messages |

### 8.4 Interactions

- **Mode switch:** Instantly updates template to mode default, updates preview
- **Template typing:** Debounced validation (150ms), live preview update
- **Invalid template:** Red border on input, error message below, preview shows last valid
- **Template with no variables:** Warning "Your template has no variables — all outputs will have the same name"
- **Merge split naming tie-in:** When "Apply naming to split output" is checked, the merge split config uses the same naming pattern

---

## 9. Subtitle Naming

### 9.1 Rule

Subtitles **always** inherit the exact base filename of their companion video, only differing in extension and optional language suffix.

```
Video output:  {base_name}.{ext}
SRT output:    {base_name}.srt
               {base_name}.{lang}.srt   (if language specified)
```

### 9.2 Multi-Language Flow

When splitting with multiple subtitle tracks:
```
Course_Part_001.mp4
Course_Part_001.en.srt
Course_Part_001.es.srt
Course_Part_001.ja.srt
```

When merging with multi-language subtitles:
```
Course_Merged.mp4
Course_Merged.en.srt
Course_Merged.es.srt
```

### 9.3 Implementation

The subtitle filename is derived from the resolved video filename:

```rust
fn derive_subtitle_path(video_path: &Path, lang: Option<&str>) -> PathBuf {
    let stem = video_path.file_stem().unwrap_or_default();
    let parent = video_path.parent().unwrap_or(Path::new("."));

    if let Some(lang) = lang {
        parent.join(format!("{}.{}.srt", stem, lang))
    } else {
        parent.join(format!("{}.srt", stem))
    }
}
```

### 9.4 Subtitle Mode Compatibility

The naming engine **must not affect** which subtitle mode is chosen or how subtitles are processed. It only controls the output filename. All five modes work identically:

| Subtitle Mode | Naming Engine Impact | Behavior |
|---------------|----------------------|----------|
| `copyAll` | Output filename used for muxed SRT track | No change — stream copied as-is |
| `extractSplit` | Output filename used for each SRT segment | SRT timecodes adjusted per segment; filename matches video |
| `exportSrt` | Output filename used for exported SRT | SRT exported separately; filename matches video |
| `embed` | Output filename used for embedded track | SRT muxed into container; filename matches video |
| `burn` | Output filename used for burned-in subtitles | Burned during re-encode; filename matches video |

**Critical:** The naming engine has **zero knowledge** of subtitle mode. It only receives the resolved video output path and derives the SRT path from it. The subtitle processing pipeline (in `split/engine.rs` and `commands/merge.rs`) continues to use its existing `SplitSubtitleMode` and `SubtitleMode` logic unchanged.

---

## 10. Backend Changes

### 10.1 New Module: `src-tauri/src/naming/`

```
src-tauri/src/naming/
├── mod.rs           // Module exports
├── parser.rs        // Template tokenization and parsing
├── resolver.rs      // Variable resolution given a context
├── validator.rs     // Template + resolved filename validation
└── context.rs       // TemplateContext builder
```

### 10.2 Template Resolution Flow

```
NamingConfig + TemplateContext
         │
         ▼
    ┌─────────┐
    │ Parser  │ ──── Token stream
    └────┬────┘
         │
         ▼
    ┌──────────┐
    │ Resolver │ ──── Resolved string
    └────┬─────┘
         │
         ▼
    ┌──────────┐
    │Validator │ ──── NamingValidation (valid + errors + preview)
    └──────────┘
```

### 10.3 Tauri Commands (New)

```rust
#[tauri::command]
fn validate_naming_template(
    template: &str,
    config: NamingConfig,
    context: TemplateContext,
) -> NamingValidation;

// Batch preview — returns first N resolved names for live preview
#[tauri::command]
fn preview_naming_batch(
    template: &str,
    config: NamingConfig,
    contexts: Vec<TemplateContext>,  // first N segment contexts
) -> Vec<String>;

// Resolve a full template given context (used by split engine)
#[tauri::command]
fn resolve_naming(
    template: &str,
    config: NamingConfig,
    context: TemplateContext,
) -> String;
```

### 10.4 Split Engine Changes

In `split/engine.rs`, replace `generate_output_filename()` with:

```rust
fn resolve_segment_name(
    input_path: &Path,
    segment: &SplitSegment,
    naming_config: &NamingConfig,
    context: &TemplateContext,
) -> PathBuf {
    let resolved = naming::resolver::resolve(
        &naming_config.template,
        naming_config,
        context,
    );
    // sanitization...
    output_dir.join(resolved)
}
```

The `NamingConfig` is added to `SplitParams` and passed through `execute_split_with_options`.

### 10.5 Merge Engine Changes

In `ffmpeg/concat.rs`, the merge output filename is already parameterized. Update to use `naming::resolver::resolve()` similarly, using `NamingConfig` from `MergeRequest.splitConfig`.

---

## 11. Default Behavior (Backward Compatibility)

The default `NamingConfig` must produce filenames **identical to the current hardcoded behavior**.

| Context | Current Pattern | Default Template | Match? |
|---------|-----------------|-------------------|--------|
| Split (parts) | `{stem} - Part {index:02}.{ext}` | `{filename} - Part {num2}.{ext}` | ✅ |
| Split (chapters) | `{stem} - Chapter {index:02}.{ext}` | `{filename} - Chapter {num2}.{ext}` | ✅ |
| Merge (count) | `{stem}_part{idx}.{ext}` | `{filename}_part{num}.{ext}` | ⚠️ Slight diff (no zero-padding, `part` lowercase) — use `{filename}_Part_{num2}` and document as opt-in |
| Merge (folder) | `{stem}_part{idx}_{folder}.{ext}` | `{filename}_Part_{num2}_{folder}.{ext}` | ⚠️ Same |

**Backward-compat approach:** The split/merge functions accept an optional `NamingConfig`. If `None`, fall back to the existing `generate_output_filename` logic. This way:
- Existing code paths (with no naming config) work exactly as today
- New code paths (with naming config) use the template engine
- The split/merge params types get a new optional field — no breaking change

---

## 12. Variable Cheat Sheet

```
┌────────────────────────────────────────────────────────────────┐
│ Click a variable to insert it at cursor position               │
├────────────────────────────────────────────────────────────────┤
│ FILE      │ {filename} {ext} {folder} {resolution}             │
│           │ {width} {height}                                    │
│ SEQUENCE  │ {num} {num2} {num3} {num4} {original_num}          │
│ TIME      │ {start} {end} {duration} {total_duration}          │
│ METADATA  │ {chapter} {part_label} {playlist} {playlist_index}  │
│ DATE/TIME │ {date} {time}                                       │
│ LANGUAGE  │ {lang} {lang_name}                                  │
│ MERGE     │ {video_count}                                       │
├────────────────────────────────────────────────────────────────┤
│ EXAMPLE: {filename}_Part_{num3} → Course_Part_001.mp4          │
│ EXAMPLE: {chapter}_{num2} → Introduction_01.mp4               │
│ EXAMPLE: {filename}_{start}_{end} → Course_00-00-00_00-30-00  │
│ EXAMPLE: {folder}_{num3} → Python_Course_001.mp4             │
│ EXAMPLE: {filename}_{date} → Course_2026-05-30.mp4            │
└────────────────────────────────────────────────────────────────┘
```

---

## 13. Migration Strategy

### Phase 1: Infrastructure (No User-Facing Changes)
1. Create `src-tauri/src/naming/` module with parser, resolver, validator
2. Add `NamingConfig` type to Rust types
3. Add `NamingConfig` type to TypeScript types
4. Add Tauri commands `validate_naming_template` and `resolve_naming`
5. Add Zustand state slices for naming config (in splitStore, mergeStore)

### Phase 2: Backend Integration (Still No UI)
6. Update split engine to accept `Option<NamingConfig>`; if `None`, use existing logic
7. Update merge engine similarly
8. Add `namingConfig` to `SplitParams` and `MergeRequest.splitConfig`
9. Write tests: template parsing, variable resolution, validation, Windows reserved names

### Phase 3: UI — Split Screen
10. Add `NamingModeSelector` component to split screen
11. Add `NamingTemplateInput` with live preview
12. Add `NamingAdvancedOptions` collapsible section
13. Wire up store actions: `setNamingMode`, `setNamingTemplate`
14. Show naming preview in `SplitPreview` segment list

### Phase 4: UI — Merge Screen
15. Add naming section to `OutputSettings.tsx`
16. Add "Apply naming to split output" and "Apply naming to subtitles" checkboxes
17. Wire up `mergeStore` naming config

### Phase 5: Polish
18. Variable cheat sheet popover/tooltip
19. Error state styling
20. Documentation in AGENTS.md

---

## 14. File-by-File Changes

### New Files
```
src-tauri/src/naming/
├── mod.rs
├── parser.rs
├── resolver.rs
├── validator.rs
└── context.rs
src/components/naming/
├── NamingModeSelector.tsx
├── NamingTemplateInput.tsx
├── NamingLivePreview.tsx
├── NamingAdvancedOptions.tsx
└── NamingCheatSheet.tsx
```

### Modified Files

**Rust:**
- `src-tauri/src/types.rs` — Add `NamingConfig`, `NamingModeId`, `NamingValidation`
- `src-tauri/src/split/types.rs` — Add `naming_config: Option<NamingConfig>` to `SplitParams`
- `src-tauri/src/split/engine.rs` — Replace `generate_output_filename` with naming resolver
- `src-tauri/src/ffmpeg/concat.rs` — Use naming resolver for merge outputs
- `src-tauri/src/commands/split.rs` — Pass `NamingConfig` through
- `src-tauri/src/commands/merge.rs` — Pass `NamingConfig` through
- `src-tauri/src/lib.rs` — Register naming module and commands

**TypeScript:**
- `src/types/index.ts` — Add `NamingConfig`, `NamingModeId`, `NamingValidation`, `NamingMode` interfaces
- `src/store/splitStore.ts` — Add naming config state and actions
- `src/store/mergeStore.ts` — Add naming config state and actions
- `src/features/split/SplitModeSelector.tsx` — Add naming section
- `src/features/split/SplitScreen.tsx` — Add naming panel
- `src/features/merge/OutputSettings.tsx` — Add naming section
- `src/constants/index.ts` — Add `NAMING_MODES` constant
- `src/utils/index.ts` — Add `formatTime`, `formatDate`, `formatTimeLocal` helpers

---

## 15. Testing Plan

### Unit Tests (Rust)
- Parser: tokenize template → correct token stream
- Parser: unknown variable → error
- Resolver: all variables resolve correctly (including new: `{folder}`, `{original_num}`, `{playlist}`, `{video_count}`, `{total_duration}`)
- Resolver: zero-padding for num/num2/num3/num4
- Resolver: `{total_duration}` formats correctly for seconds, minutes, hours
- Resolver: `{original_num}` detects leading numbers correctly
- Resolver: `{folder}` extracts parent directory name
- Validator: empty template → error
- Validator: Windows reserved name → error
- Validator: illegal chars → error
- Validator: template with no sequence variable → warning (not error)
- Integration: full template → expected output string
- Batch preview: returns first N names, total count accurate

### Unit Tests (TypeScript)
- Same test cases mirrored in TS for the frontend validator
- Cheat sheet variable list matches parser supported variables

### Integration Tests
- Split with naming config → output files match template
- Merge with naming config → output files match template
- Subtitle files get correct derived names

---

## 15.5 Pre-Implementation Audit Tests

These tests must **pass before Phase 2 (backend integration)** is considered complete. They verify the naming engine is robust against real-world edge cases.

### TEST 1 — Duplicate Prevention (1000 outputs)

```
Template: {filename}_Part_{num3}
Segments: 1000
```

**Verify:** No two outputs have the same filename. All 1000 are unique.

### TEST 2 — No-Variable Collision Detection

```
Template: {filename}
Segments: 20
```

**Verify:** Warning is shown: "Template has no sequence variable — all outputs will overwrite each other." Job is blocked.

### TEST 3 — Multi-Language Subtitle Collision

```
Template: {filename}_Part_{num3}
Languages: en, es, ja (3 tracks)
Segments: 5
```

**Verify output:**
```
Course_Part_001.mp4
Course_Part_001.en.srt  ← unique
Course_Part_001.es.srt  ← unique
Course_Part_001.ja.srt  ← unique
Course_Part_002.mp4
Course_Part_002.en.srt
...
```
No language pair produces the same filename.

### TEST 4 — Concurrent Job Isolation

```
Job A: template={filename}_Part_{num3}, outputDir=./out/
Job B: template={filename}_Part_{num3}, outputDir=./out/
Both running simultaneously.
```

**Verify:** No file written by Job A is ever overwritten by Job B. Each job's outputs remain distinct even with identical templates.

**Implementation:** Job outputs go through a job-scoped temp directory first, then moved/renamed to final location. Or: final path includes a jobId hash suffix during execution.

### TEST 5 — Windows Reserved Names

**Verify each is blocked:**
```
CON, PRN, AUX, NUL
COM1, COM2, COM3, COM4, COM5, COM6, COM7, COM8, COM9
LPT1, LPT2, LPT3, LPT4, LPT5, LPT6, LPT7, LPT8, LPT9
```

**Template producing each as stem:**
```
{filename}.mp4 where filename = "CON"
```

**Verify:** Error `RESERVED_NAME` returned. Job blocked.

### TEST 6 — Path Length Validation

**Setup:** Output directory path is 180 chars. Template resolves to 50-char filename. Total = 230 chars.

**Verify:** Error `PATH_TOO_LONG` returned. Job blocked.

### TEST 7 — Template Migration (Existing Jobs Unchanged)

**Existing job (pre-naming-engine):**
- Split job with Part count = 3, no naming config set
- Merge job with custom output path, no naming config set

**Verify after naming engine is added:**
- Existing split job produces same filenames as before (old `generate_output_filename` logic)
- Existing merge job produces same output file as before

**Implementation:** Both `SplitParams.namingConfig` and `MergeRequest.namingConfig` default to `None`. When `None`, the existing hardcoded logic is used unchanged.

### TEST 8 — Subtitle Export Modes Unaffected

For each mode, verify the naming engine does not interfere:

| Mode | Test |
|------|------|
| `copyAll` | Split with 2 subtitle tracks, mode=`copyAll`. Verify SRT files are created with correct derived names. FFmpeg stream copy works. |
| `extractSplit` | Same but mode=`extractSplit`. Verify SRT timecodes are adjusted correctly and filenames match video. |
| `exportSrt` | Verify SRT is exported standalone with matching filename. |
| `embed` | Verify SRT is muxed into container with matching filename. |
| `burn` | Verify burned subtitle video is produced with matching filename. |

---

## 15.6 Audit Summary Checklist

```
Collision Detection
  ☐ Duplicate names detected for 1000-output template
  ☐ No-variable template blocked with warning
  ☐ Multi-language subtitle filenames unique

Overwrite Prevention
  ☐ Concurrent identical templates don't overwrite
  ☐ Existing file detection works pre-job

Reserved Names
  ☐ CON, PRN, AUX, NUL blocked
  ☐ COM1-COM9, LPT1-LPT9 blocked

Path Safety
  ☐ Long filename (>200 chars) blocked
  ☐ Total path length validated

Migration
  ☐ Existing split jobs unchanged
  ☐ Existing merge jobs unchanged

Subtitle Modes
  ☐ copyAll works with naming engine
  ☐ extractSplit works with naming engine
  ☐ exportSrt works with naming engine
  ☐ embed works with naming engine
  ☐ burn works with naming engine
```

---

## 16. Open Questions / Decisions Needed

1. **Should existing split/merge jobs keep their naming?** — Yes, naming config is set at job creation time. ✅ **DECIDED**
2. **Should we provide preset "styles" (e.g., "YouTube", "Course", "Archive")?** — Nice to have, can be added as a second layer on top of modes.
3. **Should `{start}` and `{end}` format as `HH-MM-SS` or `HH:MM:SS`?** — Colon (`:`) is illegal on Windows. Use hyphen (`-`). ✅ **DECIDED**
4. **Should template be saved per-project or globally?** — Globally per user, persisted in the store. ✅ **DECIDED**
5. **Should we show a diff preview (before → after) when user changes naming mode?** — Yes, helps users understand the impact. ✅ **DECIDED**
6. **Concurrent job isolation mechanism?** — Job outputs use a jobId-scoped temp directory during execution, then moved to final location. No embedding of jobId in final output filename. ✅ **DECIDED**