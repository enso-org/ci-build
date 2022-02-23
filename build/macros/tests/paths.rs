// pub struct Parameters {
//     pub os_temp: std::path::PathBuf,
//     repo_root:   std::path::PathBuf,
// }
// impl Parameters {
//     pub fn new(
//         os_temp: impl Into<std::path::PathBuf>,
//         repo_root: impl Into<std::path::PathBuf>,
//     ) -> Self {
//         Self { os_temp: os_temp.into(), repo_root: repo_root.into() }
//     }
// }
// pub struct RepoRoot {
//     pub path:   std::path::PathBuf,
//     pub github: RepoRootGithub,
//     pub app:    RepoRootApp,
//     pub build:  RepoRootBuild,
//     pub dist:   RepoRootDist,
//     pub run:    RepoRootRun,
// }
// impl RepoRoot {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("{}", context.repo_root.display()));
//         let github = RepoRootGithub::new(context, &path);
//         let app = RepoRootApp::new(context, &path);
//         let build = RepoRootBuild::new(context, &path);
//         let dist = RepoRootDist::new(context, &path);
//         let run = RepoRootRun::new(context, &path);
//         Self { path, github, app, build, dist, run }
//     }
// }
// impl AsRef<std::path::Path> for RepoRoot {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRoot {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootGithub {
//     pub path:      std::path::PathBuf,
//     pub workflows: RepoRootGithubWorkflows,
// }
// impl RepoRootGithub {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!(".github",));
//         let workflows = RepoRootGithubWorkflows::new(context, &path);
//         Self { path, workflows }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootGithub {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootGithub {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootGithubWorkflows {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootGithubWorkflows {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("workflows",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootGithubWorkflows {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootGithubWorkflows {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootApp {
//     pub path:        std::path::PathBuf,
//     pub gui:         RepoRootAppGui,
//     pub ide_desktop: RepoRootAppIdeDesktop,
// }
// impl RepoRootApp {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("app",));
//         let gui = RepoRootAppGui::new(context, &path);
//         let ide_desktop = RepoRootAppIdeDesktop::new(context, &path);
//         Self { path, gui, ide_desktop }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootApp {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootApp {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootAppGui {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootAppGui {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("gui",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootAppGui {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootAppGui {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootAppIdeDesktop {
//     pub path: std::path::PathBuf,
//     pub lib:  RepoRootAppIdeDesktopLib,
// }
// impl RepoRootAppIdeDesktop {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("ide-desktop",));
//         let lib = RepoRootAppIdeDesktopLib::new(context, &path);
//         Self { path, lib }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootAppIdeDesktop {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootAppIdeDesktop {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootAppIdeDesktopLib {
//     pub path:            std::path::PathBuf,
//     pub content:         RepoRootAppIdeDesktopLibContent,
//     pub project_manager: RepoRootAppIdeDesktopLibProjectManager,
// }
// impl RepoRootAppIdeDesktopLib {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("lib",));
//         let content = RepoRootAppIdeDesktopLibContent::new(context, &path);
//         let project_manager = RepoRootAppIdeDesktopLibProjectManager::new(context, &path);
//         Self { path, content, project_manager }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootAppIdeDesktopLib {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootAppIdeDesktopLib {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootAppIdeDesktopLibContent {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootAppIdeDesktopLibContent {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("content",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootAppIdeDesktopLibContent {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootAppIdeDesktopLibContent {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootAppIdeDesktopLibProjectManager {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootAppIdeDesktopLibProjectManager {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("project-manager",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootAppIdeDesktopLibProjectManager {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootAppIdeDesktopLibProjectManager {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootBuild {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootBuild {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("build",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootBuild {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootBuild {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDist {
//     pub path:       std::path::PathBuf,
//     pub bin:        RepoRootDistBin,
//     pub client:     RepoRootDistClient,
//     pub content:    RepoRootDistContent,
//     pub tmp:        RepoRootDistTmp,
//     pub wasm:       RepoRootDistWasm,
//     pub init:       RepoRootDistInit,
//     pub build_init: RepoRootDistBuildInit,
//     pub build_json: RepoRootDistBuildJson,
// }
// impl RepoRootDist {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("dist",));
//         let bin = RepoRootDistBin::new(context, &path);
//         let client = RepoRootDistClient::new(context, &path);
//         let content = RepoRootDistContent::new(context, &path);
//         let tmp = RepoRootDistTmp::new(context, &path);
//         let wasm = RepoRootDistWasm::new(context, &path);
//         let init = RepoRootDistInit::new(context, &path);
//         let build_init = RepoRootDistBuildInit::new(context, &path);
//         let build_json = RepoRootDistBuildJson::new(context, &path);
//         Self { path, bin, client, content, tmp, wasm, init, build_init, build_json }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDist {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDist {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistBin {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistBin {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("bin",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistBin {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistBin {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistClient {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistClient {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("client",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistClient {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistClient {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistContent {
//     pub path:         std::path::PathBuf,
//     pub assets:       RepoRootDistContentAssets,
//     pub package_json: RepoRootDistContentPackageJson,
//     pub preload_js:   RepoRootDistContentPreloadJs,
// }
// impl RepoRootDistContent {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("content",));
//         let assets = RepoRootDistContentAssets::new(context, &path);
//         let package_json = RepoRootDistContentPackageJson::new(context, &path);
//         let preload_js = RepoRootDistContentPreloadJs::new(context, &path);
//         Self { path, assets, package_json, preload_js }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistContent {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistContent {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistContentAssets {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistContentAssets {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("assets",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistContentAssets {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistContentAssets {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistContentPackageJson {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistContentPackageJson {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("package.json",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistContentPackageJson {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistContentPackageJson {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistContentPreloadJs {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistContentPreloadJs {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("preload.js",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistContentPreloadJs {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistContentPreloadJs {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistTmp {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistTmp {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("tmp",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistTmp {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistTmp {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistWasm {
//     pub path:        std::path::PathBuf,
//     pub ide_wasm:    RepoRootDistWasmIdeWasm,
//     pub ide_bg_wasm: RepoRootDistWasmIdeBgWasm,
//     pub ide_js:      RepoRootDistWasmIdeJs,
// }
// impl RepoRootDistWasm {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("wasm",));
//         let ide_wasm = RepoRootDistWasmIdeWasm::new(context, &path);
//         let ide_bg_wasm = RepoRootDistWasmIdeBgWasm::new(context, &path);
//         let ide_js = RepoRootDistWasmIdeJs::new(context, &path);
//         Self { path, ide_wasm, ide_bg_wasm, ide_js }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistWasm {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistWasm {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistWasmIdeWasm {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistWasmIdeWasm {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("ide.wasm",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistWasmIdeWasm {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistWasmIdeWasm {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistWasmIdeBgWasm {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistWasmIdeBgWasm {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("ide_bg.wasm",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistWasmIdeBgWasm {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistWasmIdeBgWasm {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistWasmIdeJs {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistWasmIdeJs {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("ide.js",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistWasmIdeJs {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistWasmIdeJs {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistInit {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistInit {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("init",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistInit {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistInit {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistBuildInit {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistBuildInit {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("build-init",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistBuildInit {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistBuildInit {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootDistBuildJson {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootDistBuildJson {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("build.json",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootDistBuildJson {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootDistBuildJson {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
// pub struct RepoRootRun {
//     pub path: std::path::PathBuf,
// }
// impl RepoRootRun {
//     pub fn new(context: &Parameters, parent: &std::path::Path) -> Self {
//         let path = parent.join(format!("run",));
//         Self { path }
//     }
// }
// impl AsRef<std::path::Path> for RepoRootRun {
//     fn as_ref(&self) -> &std::path::Path {
//         &self.path
//     }
// }
// impl std::ops::Deref for RepoRootRun {
//     type Target = std::path::PathBuf;
//     fn deref(&self) -> &Self::Target {
//         &self.path
//     }
// }
