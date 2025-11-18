# GitHub Actions Workflows

This directory contains GitHub Actions workflows for the webtor-rs project.

## Workflows

### Build and Deploy to GitHub Pages (`build-and-deploy.yml`)

This workflow automatically builds the webtor-demo project and deploys it to GitHub Pages.

**Triggers:**
- Push to `main` or `master` branches
- Pull requests to `main` or `master` branches
- Manual workflow dispatch

**Process:**
1. Sets up Rust toolchain with WASM target
2. Installs wasm-pack
3. Builds webtor-wasm and webtor-demo WASM modules
4. Copies built files to the static directory
5. Deploys to GitHub Pages (only on main/master branch pushes)

**Requirements:**
- Repository must have GitHub Pages enabled
- Source should be set to "GitHub Actions" in repository settings

**Deployment URL:**
The demo will be available at `https://igor53627.github.io/webtor-rs/`

## Setup Instructions

1. Ensure GitHub Pages is enabled in your repository settings:
   - Go to Settings → Pages
   - Set Source to "GitHub Actions"

2. The workflow will automatically run on pushes to main/master branches

3. To manually trigger a deployment:
   - Go to Actions tab
   - Select "Build and Deploy to GitHub Pages"
   - Click "Run workflow"