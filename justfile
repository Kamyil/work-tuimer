# WorkTimer build recipes

# Display all available recipes
default:
    @just --list

# Build the project in release mode
build:
    cargo build --release

# Run tests
test:
    cargo test

# Run clippy linting
lint:
    cargo clippy -- -D warnings

# Check code formatting
fmt-check:
    cargo fmt -- --check

# Format code
fmt:
    cargo fmt

# Create a full release: bump version, sync AUR metadata, commit, tag, push, and publish to cargo
# Usage: just release v0.3.2
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    
    AUR_REPO_PATH="../aur-work-tuimer"
    
    # Validate version format (v followed by semver)
    if ! [[ "{{version}}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
        echo "Invalid version format: {{version}}"
        echo "✓ Expected format: v0.1.0 or v0.1.0-rc1"
        exit 1
    fi
    
    # Check for uncommitted changes in the main repo
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "You have uncommitted changes. Please commit or stash them first."
        exit 1
    fi
    
    CURRENT_BRANCH="$(git branch --show-current)"
    if [[ "$CURRENT_BRANCH" != "main" ]]; then
        echo "Release must be run from the local main branch"
        exit 1
    fi
    
    git fetch origin --tags
    
    if ! git rev-parse --verify origin/main >/dev/null 2>&1; then
        echo "Remote branch origin/main is not available"
        exit 1
    fi
    
    read -r MAIN_AHEAD MAIN_BEHIND <<< "$(git rev-list --left-right --count HEAD...origin/main)"
    if [[ "$MAIN_BEHIND" -ne 0 ]]; then
        echo "Local main is behind origin/main by $MAIN_BEHIND commit(s). Pull or rebase before releasing."
        exit 1
    fi
    
    # Check if tag already exists
    if git rev-parse "{{version}}" >/dev/null 2>&1; then
        echo "Tag {{version}} already exists"
        exit 1
    fi
    
    # Check AUR repo availability and cleanliness
    if [[ ! -d "$AUR_REPO_PATH" ]]; then
        echo "AUR repo not found at $AUR_REPO_PATH"
        exit 1
    fi
    
    if ! git -C "$AUR_REPO_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        echo "AUR repo path is not a git repository: $AUR_REPO_PATH"
        exit 1
    fi
    
    if [[ -n "$(git -C "$AUR_REPO_PATH" status --porcelain)" ]]; then
        echo "AUR repo has uncommitted changes. Please commit or stash them first: $AUR_REPO_PATH"
        exit 1
    fi
    
    AUR_ORIGINAL_BRANCH="$(git -C "$AUR_REPO_PATH" branch --show-current)"
    if [[ -z "$AUR_ORIGINAL_BRANCH" ]]; then
        echo "AUR repo must be on a local branch: $AUR_REPO_PATH"
        exit 1
    fi
    
    cleanup_aur_branch() {
        if [[ "$(git -C "$AUR_REPO_PATH" branch --show-current)" != "$AUR_ORIGINAL_BRANCH" ]]; then
            git -C "$AUR_REPO_PATH" checkout "$AUR_ORIGINAL_BRANCH" >/dev/null 2>&1 || true
        fi
    }
    trap cleanup_aur_branch EXIT
    
    # Extract version without 'v' prefix
    VERSION="{{version}}"
    VERSION="${VERSION#v}"
    echo "📦 Preparing release {{version}} (version: $VERSION)..."
    
    # Bump version in Cargo.toml
    echo "📝 Bumping version in Cargo.toml to $VERSION..."
    sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
    rm Cargo.toml.bak
    
    # Verify the change
    if ! grep -q "version = \"$VERSION\"" Cargo.toml; then
        echo "Failed to update version in Cargo.toml"
        exit 1
    fi
    echo "✓ Version updated in Cargo.toml"
    
    # Update AUR packaging files in the main repo
    echo "📝 Updating AUR packaging metadata..."
    sed -i.bak "s/^pkgver=.*/pkgver=$VERSION/" packaging/aur/PKGBUILD
    rm packaging/aur/PKGBUILD.bak
    printf '%b\n' \
        'pkgbase = work-tuimer' \
        '\tpkgdesc = Simple, keyboard-driven TUI for time-tracking' \
        "\tpkgver = $VERSION" \
        '\tpkgrel = 1' \
        '\turl = https://github.com/Kamyil/work-tuimer' \
        '\tarch = x86_64' \
        '\tarch = aarch64' \
        '\tlicense = MIT' \
        '\tmakedepends = cargo' \
        '\tmakedepends = rust' \
        '\tdepends = gcc-libs' \
        '\tdepends = glibc' \
        '\toptions = !lto' \
        "\tsource = work-tuimer-$VERSION.tar.gz::https://github.com/Kamyil/work-tuimer/archive/refs/tags/v$VERSION.tar.gz" \
        '\tb2sums = SKIP' \
        '' \
        'pkgname = work-tuimer' \
        > packaging/aur/.SRCINFO
    echo "✓ AUR metadata updated in packaging/aur"
    
    # Run tests to ensure everything works
    echo "Running tests..."
    nix-shell --run "cargo test --quiet"
    echo "✓ Tests passed"
    
    # Sync AUR packaging files to the AUR repo
    cp packaging/aur/PKGBUILD "$AUR_REPO_PATH/PKGBUILD"
    cp packaging/aur/.SRCINFO "$AUR_REPO_PATH/.SRCINFO"
    echo "✓ Synced AUR metadata to $AUR_REPO_PATH"
    
    # Commit version bump
    echo "Committing version bump..."
    git add Cargo.toml Cargo.lock packaging/aur/PKGBUILD packaging/aur/.SRCINFO
    git commit -m "Bump version to $VERSION"
    echo "✓ Version bump committed"
    
    # Create and push tag
    echo " Creating git tag {{version}}..."
    git tag "{{version}}"
    echo "✓ Tag created"
    
    echo "Pushing to remote..."
    git push origin main
    git push origin "{{version}}"
    echo "✓ Changes and tag pushed"
    
    # Commit and push AUR repo changes after the release tag is available
    echo "Committing AUR package update..."
    if [[ "$AUR_ORIGINAL_BRANCH" != "master" ]]; then
        git -C "$AUR_REPO_PATH" checkout master
        git -C "$AUR_REPO_PATH" pull --ff-only origin master
    fi
    git -C "$AUR_REPO_PATH" add PKGBUILD .SRCINFO
    git -C "$AUR_REPO_PATH" commit -m "Update to v$VERSION"
    git -C "$AUR_REPO_PATH" push origin master
    echo "✓ AUR repo updated"
    
    # Publish to crates.io
    echo "Publishing to crates.io..."
    nix-shell --run "cargo publish"
    echo "✓ Published to crates.io"
    
    echo ""
    echo "Release {{version}} complete!"
    echo "GitHub Actions will now build and publish pre-built binaries"
    echo "Watch progress at: https://github.com/$(git config --get remote.origin.url | sed 's/.*:\(.*\)\.git/\1/')/actions"
    echo "Crate available at: https://crates.io/crates/work-tuimer"

# Dry-run release: check what would happen without making changes
release-check version:
    #!/usr/bin/env bash
    set -euo pipefail
    
    AUR_REPO_PATH="../aur-work-tuimer"
    
    # Validate version format
    if ! [[ "{{version}}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
        echo "❌ Invalid version format: {{version}}"
        exit 1
    fi
    
    VERSION="{{version}}"
    VERSION="${VERSION#v}"
    CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    CURRENT_BRANCH="$(git branch --show-current)"
    
    git fetch origin --tags >/dev/null 2>&1
    
    echo "🔍 Release check for {{version}}"
    echo ""
    echo "Current version: $CURRENT_VERSION"
    echo "New version:     $VERSION"
    echo ""
    
    # Check main repo status
    if [[ -z "$(git status --porcelain)" ]]; then
        echo "✓ No uncommitted changes"
    else
        echo "❌ Uncommitted changes detected"
    fi
    
    # Check main branch sync status
    if [[ "$CURRENT_BRANCH" != "main" ]]; then
        echo "❌ Current branch is $CURRENT_BRANCH; run releases from main"
    elif ! git rev-parse --verify origin/main >/dev/null 2>&1; then
        echo "❌ origin/main is not available"
    else
        read -r MAIN_AHEAD MAIN_BEHIND <<< "$(git rev-list --left-right --count HEAD...origin/main)"
        if [[ "$MAIN_BEHIND" -eq 0 ]]; then
            if [[ "$MAIN_AHEAD" -eq 0 ]]; then
                echo "✓ Local main matches origin/main"
            else
                echo "✓ Local main is ahead of origin/main by $MAIN_AHEAD commit(s)"
            fi
        elif [[ "$MAIN_AHEAD" -eq 0 ]]; then
            echo "❌ Local main is behind origin/main by $MAIN_BEHIND commit(s)"
        else
            echo "❌ Local main has diverged from origin/main (ahead $MAIN_AHEAD, behind $MAIN_BEHIND)"
        fi
    fi
    
    # Check AUR repo status
    if [[ -d "$AUR_REPO_PATH" ]] && git -C "$AUR_REPO_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        if [[ -z "$(git -C "$AUR_REPO_PATH" status --porcelain)" ]]; then
            echo "✓ AUR repo is clean: $AUR_REPO_PATH"
        else
            echo "❌ AUR repo has uncommitted changes: $AUR_REPO_PATH"
        fi
    else
        echo "❌ AUR repo not available: $AUR_REPO_PATH"
    fi
    
    # Check if tag exists
    if git rev-parse "{{version}}" >/dev/null 2>&1; then
        echo "❌ Tag {{version}} already exists"
    else
        echo "✓ Tag {{version}} is available"
    fi
    
    # Check cargo login
    echo ""
    echo "Checking cargo registry authentication..."
    if cargo login --help >/dev/null 2>&1; then
        echo "✓ Cargo is available"
    else
        echo "❌ Cargo not found"
    fi
