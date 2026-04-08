//! Per-template builders.
//!
//! The bash pipeline has 11 `builder/*.sh` scripts that are 90% identical
//! boilerplate with small per-template tweaks. Instead of porting 11
//! near-duplicate Rust files, we express each builder as a [`BuilderSpec`]
//! and let a generic [`apply_spec`] function execute them all.
//!
//! If a template's logic doesn't fit the spec (currently: `go-modular` has
//! doc/web cleanups, `shared-ui` has a copy-from-packages step), we add
//! targeted closures via [`BuilderSpec::custom`].

use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::common::{
    keep_only, modify_json_file, prepend_to_files_with_ext, rename_ext_to_raw, rename_one_to_raw,
    replace_in_file, replace_in_files,
};

/// Immutable spec for a single template build.
///
/// Every field here was derived from reading the corresponding
/// `builder/<name>.sh` shell script verbatim. Any divergence would
/// violate the canonical-equivalence contract (AC5/6 of the spec).
pub struct BuilderSpec {
    /// Name used for CLI lookup (`templates-cli builder <name>`). Matches
    /// the shell-script basename without the `.sh` suffix.
    pub name: &'static str,

    /// Source directory name under `templates/` before rename. Usually the
    /// same as the target, but differs for astro (`astro-web` → `astro`),
    /// nextjs (`nextjs-app` → `nextjs`), etc.
    pub source_name: &'static str,

    /// Target directory name under `templates/`. The final name users see
    /// in `moon generate`.
    pub target_name: &'static str,

    /// Human-readable banner printed when the builder runs. Matches the
    /// "Building X project templates..." echo at the top of each shell script.
    pub banner: &'static str,

    /// If set, every file in the target directory that contains this
    /// literal port string is rewritten with `{{ port_number }}`.
    pub default_port: Option<&'static str>,

    /// Per-template file-extension renames (e.g. `.py` → `.raw.py`).
    pub rename_exts_to_raw: &'static [&'static str],

    /// Single-file renames (relative to target dir). Used for things like
    /// `.mockery.yml` → `.mockery.raw.yml` and specific test files.
    pub rename_single_files: &'static [&'static str],

    /// Whether to remove `@repo/shared-ui` from `package.json` and
    /// `tsconfig.json` via JSON surgery. Corresponds to the 5 frontend
    /// builders that currently `jq` this out.
    pub strip_shared_ui_refs: bool,

    /// Whether to prepend `---\nforce: true\n---\n` to `.astro` files
    /// before renaming them. Astro-specific.
    pub astro_frontmatter_prepend: bool,

    /// Optional custom pass, run AFTER all the declarative steps above.
    /// Receives the absolute target path.
    pub custom: Option<fn(target: &Path) -> Result<()>>,
}

/// The full registry. Order matches the order in `build-templates.sh`.
pub const REGISTRY: &[&BuilderSpec] = &[
    &ASTRO,
    &EXPO_APP,
    &FASTAPI_AI,
    &GO_CLEAN,
    &GO_MODULAR,
    &NEXTJS_APP,
    &REACT_APP,
    &REACT_SSR,
    &STRAPI_CMS,
    &TANSTACK_START,
    &SHARED_UI,
];

/// Look up a builder by name and run it against `root`.
pub fn run_builder(name: &str, root: &Path) -> Result<()> {
    let spec = REGISTRY
        .iter()
        .find(|s| s.name == name)
        .copied()
        .ok_or_else(|| anyhow!("unknown builder: {name}"))?;
    apply_spec(spec, root)
}

/// Run every builder in the registry, in declaration order.
pub fn run_all(root: &Path) -> Result<()> {
    for spec in REGISTRY {
        apply_spec(spec, root)?;
    }
    Ok(())
}

/// Execute a [`BuilderSpec`] against a monorepo root.
///
/// The order of operations mirrors the shell-script order EXACTLY:
/// 1. If source directory exists AND source != target, rename source → target
/// 2. Replace placeholders in `moon.yml` (source name, description)
/// 3. Replace port number across all files (except `template.yml`)
/// 4. Replace source name across all files (except `template.yml`)
/// 5. Replace `_CHANGE_ME_DESCRIPTION_` across all files (except `template.yml`)
/// 6. Astro frontmatter prepend (astro only)
/// 7. File-extension renames (.py → .raw.py, .astro → .raw.astro, etc.)
/// 8. Single-file renames (.mockery.yml → .mockery.raw.yml, etc.)
/// 9. Strip `@repo/shared-ui` from package.json + tsconfig.json (if enabled)
/// 10. Custom pass (if any)
pub fn apply_spec(spec: &BuilderSpec, root: &Path) -> Result<()> {
    tracing::info!("{}", spec.banner);

    let templates_dir = root.join("templates");
    let source_path = templates_dir.join(spec.source_name);
    let target_path = templates_dir.join(spec.target_name);

    // Step 1: source → target rename when they differ.
    if source_path.exists() && spec.source_name != spec.target_name {
        fs_err::rename(&source_path, &target_path).with_context(|| {
            format!(
                "rename source -> target: {} -> {}",
                source_path.display(),
                target_path.display()
            )
        })?;
    }

    if !target_path.exists() {
        tracing::warn!(
            "target dir does not exist, skipping: {}",
            target_path.display()
        );
        return Ok(());
    }

    // Step 2: moon.yml placeholder replacement (strict order — moon.yml first).
    let moon_yml = target_path.join("moon.yml");
    if moon_yml.exists() {
        replace_in_file(
            &moon_yml,
            spec.source_name,
            "{{ package_name | kebab_case }}",
        )?;
        replace_in_file(
            &moon_yml,
            "_CHANGE_ME_DESCRIPTION_",
            "{{ package_description }}",
        )?;
    }

    // Step 3: port number replacement across all files (bash order: port first,
    // then source name, then description — we preserve that exactly).
    if let Some(port) = spec.default_port {
        replace_in_files(&target_path, port, "{{ port_number }}")?;
    }

    // Step 4: source name replacement across all files.
    replace_in_files(
        &target_path,
        spec.source_name,
        "{{ package_name | kebab_case }}",
    )?;

    // Step 5: description placeholder replacement across all files.
    replace_in_files(
        &target_path,
        "_CHANGE_ME_DESCRIPTION_",
        "{{ package_description }}",
    )?;

    // Step 6: astro frontmatter prepend (must run BEFORE the .astro rename).
    if spec.astro_frontmatter_prepend {
        prepend_to_files_with_ext(&target_path, "astro", "---\nforce: true\n---\n")?;
    }

    // Step 7: file-extension renames.
    for ext in spec.rename_exts_to_raw {
        rename_ext_to_raw(&target_path, ext)?;
    }

    // Step 8: single-file renames.
    for rel in spec.rename_single_files {
        rename_one_to_raw(&target_path.join(rel))?;
    }

    // Step 9: strip @repo/shared-ui references.
    if spec.strip_shared_ui_refs {
        strip_shared_ui(&target_path)?;
    }

    // Step 10: custom pass.
    if let Some(custom) = spec.custom {
        custom(&target_path)?;
    }

    Ok(())
}

/// Remove `@repo/shared-ui` references from `package.json` and `tsconfig.json`.
///
/// Mirrors the `jq` invocations in the 5 frontend builders. Handles three
/// locations the shared-ui key can appear in:
///
/// - `package.json`: `.dependencies["@repo/shared-ui"]`
/// - `tsconfig.json`: `.references[] | select(.path == "../../packages/shared-ui")`
/// - `package.json` (tanstack variant): `.dependencies["@repo/shared-ui"]`
///
/// All three are handled with defensive access so the builder is idempotent.
fn strip_shared_ui(target: &Path) -> Result<()> {
    let package_json = target.join("package.json");
    modify_json_file(&package_json, |value| {
        if let Some(deps) = value
            .get_mut("dependencies")
            .and_then(|d| d.as_object_mut())
        {
            deps.remove("@repo/shared-ui");
        }
        if let Some(deps) = value
            .get_mut("devDependencies")
            .and_then(|d| d.as_object_mut())
        {
            deps.remove("@repo/shared-ui");
        }
        Ok(())
    })?;

    let tsconfig_json = target.join("tsconfig.json");
    modify_json_file(&tsconfig_json, |value| {
        if let Some(refs) = value.get_mut("references").and_then(|r| r.as_array_mut()) {
            refs.retain(|r| {
                r.get("path").and_then(|p| p.as_str()) != Some("../../packages/shared-ui")
            });
        }
        Ok(())
    })?;
    Ok(())
}

// -----------------------------------------------------------------------
// Per-template specs below. Each one is a straight translation of the
// corresponding shell script. The comments cite the source script and any
// non-obvious ports.
// -----------------------------------------------------------------------

/// `builder/astro.sh`: astro-web → astro, port 4321, frontmatter prepend,
/// .astro → .raw.astro.
pub const ASTRO: BuilderSpec = BuilderSpec {
    name: "astro",
    source_name: "astro-web",
    target_name: "astro",
    banner: "Building Astro project templates...",
    default_port: Some("4321"),
    rename_exts_to_raw: &["astro"],
    rename_single_files: &[],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: true,
    custom: None,
};

/// `builder/expo-app.sh`: expo-app → expo, no port replacement.
///
/// Note: the original script does NOT do a port replacement because expo's
/// default dev port (19000/19006) isn't in the source files as a hardcoded
/// literal. We preserve that by setting `default_port = None`.
pub const EXPO_APP: BuilderSpec = BuilderSpec {
    name: "expo-app",
    source_name: "expo-app",
    target_name: "expo",
    banner: "Building Expo project templates...",
    default_port: None,
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/fastapi-ai.sh`: fastapi-ai (same name), port 8080, .py → .raw.py.
pub const FASTAPI_AI: BuilderSpec = BuilderSpec {
    name: "fastapi-ai",
    source_name: "fastapi-ai",
    target_name: "fastapi-ai",
    banner: "Building FastAPI-AI project templates...",
    default_port: Some("8080"),
    rename_exts_to_raw: &["py"],
    rename_single_files: &[],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/go-clean.sh`: go-clean (same name), port 8000,
/// .mockery.yml → .mockery.raw.yml.
pub const GO_CLEAN: BuilderSpec = BuilderSpec {
    name: "go-clean",
    source_name: "go-clean",
    target_name: "go-clean",
    banner: "Building Go Clean Architecture project templates...",
    default_port: Some("8000"),
    rename_exts_to_raw: &[],
    rename_single_files: &[".mockery.yml"],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/go-modular.sh`: go-modular (same name), port 8000,
/// multiple single-file renames, docs/ and web/ cleanups via custom pass.
pub const GO_MODULAR: BuilderSpec = BuilderSpec {
    name: "go-modular",
    source_name: "go-modular",
    target_name: "go-modular",
    banner: "Building Go Modular project templates...",
    default_port: Some("8000"),
    rename_exts_to_raw: &[],
    rename_single_files: &[".mockery.yml", "internal/notification/mailer_test.go"],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: false,
    custom: Some(go_modular_custom),
};

fn go_modular_custom(target: &Path) -> Result<()> {
    // Rename templates/emails/*.html to *.raw.html (only that subdirectory).
    let emails = target.join("templates/emails");
    if emails.exists() {
        rename_ext_to_raw(&emails, "html")?;
    }
    // Keep only docs/embed.go
    keep_only(&target.join("docs"), &["embed.go"])?;
    // Keep only web/embed.go and web/static/index.html
    keep_only(&target.join("web"), &["embed.go", "static/index.html"])?;
    Ok(())
}

/// `builder/nextjs-app.sh`: nextjs-app → nextjs, port 3200, strip shared-ui.
pub const NEXTJS_APP: BuilderSpec = BuilderSpec {
    name: "nextjs-app",
    source_name: "nextjs-app",
    target_name: "nextjs",
    banner: "Building Next.js project templates...",
    default_port: Some("3200"),
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: true,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/react-app.sh`: react-app (same name), port 3000, strip shared-ui.
pub const REACT_APP: BuilderSpec = BuilderSpec {
    name: "react-app",
    source_name: "react-app",
    target_name: "react-app",
    banner: "Building React SPA project templates...",
    default_port: Some("3000"),
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: true,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/react-ssr.sh`: react-ssr (same name), port 3100, strip shared-ui.
pub const REACT_SSR: BuilderSpec = BuilderSpec {
    name: "react-ssr",
    source_name: "react-ssr",
    target_name: "react-ssr",
    banner: "Building React SSR project templates...",
    default_port: Some("3100"),
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: true,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/strapi-cms.sh`: strapi-cms → strapi, port 1337, strip shared-ui.
pub const STRAPI_CMS: BuilderSpec = BuilderSpec {
    name: "strapi-cms",
    source_name: "strapi-cms",
    target_name: "strapi",
    banner: "Building Strapi CMS project templates...",
    default_port: Some("1337"),
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: true,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/tanstack-start.sh`: tanstack-start (same name), port 3300,
/// strip shared-ui.
pub const TANSTACK_START: BuilderSpec = BuilderSpec {
    name: "tanstack-start",
    source_name: "tanstack-start",
    target_name: "tanstack-start",
    banner: "Building TanStack Start project templates...",
    default_port: Some("3300"),
    rename_exts_to_raw: &[],
    rename_single_files: &[],
    strip_shared_ui_refs: true,
    astro_frontmatter_prepend: false,
    custom: None,
};

/// `builder/shared-ui.sh`: special — copies from `packages/shared-ui` into
/// `templates/shared-ui`, cleans `node_modules/dist`, renames .tsx/.mdx to
/// their `.raw.*` variants.
pub const SHARED_UI: BuilderSpec = BuilderSpec {
    name: "shared-ui",
    source_name: "shared-ui",
    target_name: "shared-ui",
    banner: "Building Shared UI project templates...",
    default_port: None,
    rename_exts_to_raw: &["tsx", "mdx"],
    rename_single_files: &[],
    strip_shared_ui_refs: false,
    astro_frontmatter_prepend: false,
    custom: Some(shared_ui_custom),
};

#[allow(clippy::unnecessary_wraps)] // signature must match BuilderSpec::custom
fn shared_ui_custom(_target: &Path) -> Result<()> {
    // The bash script for shared-ui has additional copy-from-packages
    // semantics that require access to the root (not just target). Phase A
    // will land a follow-up task to wire this custom pass through the
    // monorepo root — for now we log a warning and return Ok so the
    // builder pipeline still runs end-to-end against the rest of the
    // templates.
    tracing::warn!(
        "shared-ui builder custom pass not yet implemented \
         (copy from packages/shared-ui and deep cleanup); \
         follow-up Task A-SU-1 in the ralplan"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_eleven_builders() {
        let names: Vec<&str> = REGISTRY.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 11);
        assert!(names.contains(&"astro"));
        assert!(names.contains(&"expo-app"));
        assert!(names.contains(&"fastapi-ai"));
        assert!(names.contains(&"go-clean"));
        assert!(names.contains(&"go-modular"));
        assert!(names.contains(&"nextjs-app"));
        assert!(names.contains(&"react-app"));
        assert!(names.contains(&"react-ssr"));
        assert!(names.contains(&"strapi-cms"));
        assert!(names.contains(&"tanstack-start"));
        assert!(names.contains(&"shared-ui"));
    }

    #[test]
    fn registry_names_are_unique() {
        let mut names: Vec<&str> = REGISTRY.iter().map(|s| s.name).collect();
        names.sort_unstable();
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, names.len(), "builder names must be unique");
    }

    #[test]
    fn frontend_builders_strip_shared_ui() {
        for spec in REGISTRY {
            if matches!(
                spec.name,
                "react-app" | "react-ssr" | "nextjs-app" | "strapi-cms" | "tanstack-start"
            ) {
                assert!(
                    spec.strip_shared_ui_refs,
                    "{} must strip @repo/shared-ui refs",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn only_astro_has_frontmatter_prepend() {
        for spec in REGISTRY {
            if spec.name == "astro" {
                assert!(spec.astro_frontmatter_prepend);
            } else {
                assert!(!spec.astro_frontmatter_prepend);
            }
        }
    }
}
