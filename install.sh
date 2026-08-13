#!/bin/sh
# shellcheck shell=dash
# shellcheck disable=SC2039  # `local` is non-POSIX but universally available

set -eu

REPO="imduchuyyy/agentcanpay"
BIN_DIR="${AGENTCANPAY_BIN_DIR:-${HOME}/.agentcanpay/bin}"
IGNORE_VERIFICATION="${AGENTCANPAY_IGNORE_VERIFICATION:-false}"
QUIET=no

usage() {
    cat <<EOF
The installer for agentcanpay

Usage: install.sh [OPTIONS]

Options:
  -q, --quiet   Only print errors
  -f, --force   Skip verification of the download (INSECURE)
  -h, --help    Print help

Environment variables:
  AGENTCANPAY_VERSION               Install this version instead of the latest
  AGENTCANPAY_BIN_DIR               Install here instead of ~/.agentcanpay/bin
  AGENTCANPAY_IGNORE_VERIFICATION   Skip verification when set to "true"
  AGENTCANPAY_NO_SKILL              Do not install the agent skill file
EOF
}

main() {
    for arg in "$@"; do
        case "$arg" in
            -h|--help) usage; exit 0 ;;
            -q|--quiet) QUIET=yes ;;
            -f|--force) IGNORE_VERIFICATION=true ;;
            *) err "unknown option: $arg" ;;
        esac
    done

    need_cmd uname
    need_cmd mktemp
    need_cmd mkdir
    need_cmd mv
    need_cmd rm
    need_cmd tar
    need_cmd chmod
    downloader --check

    local _target
    _target="$(detect_target)"

    local _asset="agentcanpay-${_target}.tar.gz"
    local _base
    if [ "${AGENTCANPAY_VERSION+set}" = set ]; then
        _base="https://github.com/${REPO}/releases/download/v${AGENTCANPAY_VERSION#v}"
        say "installing agentcanpay ${AGENTCANPAY_VERSION#v} for ${_target}"
    else
        _base="https://github.com/${REPO}/releases/latest/download"
        say "installing the latest agentcanpay for ${_target}"
    fi

    local _dir
    _dir="$(mktemp -d)" || err "could not create a temporary directory"
    # shellcheck disable=SC2064  # _dir is expanded now on purpose
    trap "rm -rf \"$_dir\"" EXIT

    local _archive="${_dir}/${_asset}"
    download "${_base}/${_asset}" "$_archive" "$_target"
    verify "$_base" "$_asset" "$_archive" "$_dir"

    # --strip-components drops the versioned directory inside the archive,
    # so the binary lands directly in the temp dir.
    tar xzf "$_archive" -C "$_dir" --strip-components=1 \
        || err "could not unpack $_asset"
    [ -f "${_dir}/agentcanpay" ] || err "$_asset did not contain a binary"
    chmod +x "${_dir}/agentcanpay"

    # /tmp is mounted noexec on some hardened hosts, and the failure would
    # otherwise surface later as a confusing "not found" from the shell.
    if [ ! -x "${_dir}/agentcanpay" ]; then
        err "cannot mark the binary executable, likely a noexec temp dir.
set AGENTCANPAY_BIN_DIR and re-run, or unpack $_asset by hand"
    fi

    install_binary "${_dir}/agentcanpay"
    install_skill
    post_install
}

# Installing the binary without the skill leaves an agent with no way to
# know it exists, so this is part of installing rather than a second step.
# Best-effort on purpose: a home directory the skill cannot be written to
# is not a reason to fail an install that already succeeded.
install_skill() {
    if [ "${AGENTCANPAY_NO_SKILL:-false}" = true ]; then
        return 0
    fi
    if ! "${BIN_DIR}/agentcanpay" setup > /dev/null; then
        warn "installed the binary, but could not install the skill.
run '${BIN_DIR}/agentcanpay setup' to see why"
    fi
}

install_binary() {
    local _src="$1"
    mkdir -p "$BIN_DIR" || err "could not create $BIN_DIR"
    [ -w "$BIN_DIR" ] || err "$BIN_DIR is not writable by this user"

    # Staged inside BIN_DIR, never in the temp dir: the final step has to be
    # a rename on one filesystem, both so it is atomic and so it can replace
    # a binary that is running right now — which is what `update` does.
    local _staged="${BIN_DIR}/.agentcanpay.new.$$"
    cp "$_src" "$_staged" || err "could not write to $BIN_DIR"
    chmod 755 "$_staged"
    mv -f "$_staged" "${BIN_DIR}/agentcanpay" \
        || { rm -f "$_staged"; err "could not replace ${BIN_DIR}/agentcanpay"; }
}

verify() {
    local _base="$1"
    local _asset="$2"
    local _archive="$3"
    local _dir="$4"

    if [ "$IGNORE_VERIFICATION" = true ]; then
        warn "skipping verification of the download"
        return 0
    fi

    # A checksum fetched from the same host as the binary only rules out a
    # corrupt transfer. It runs first because it is cheap and needs nothing
    # installed; the attestation below is the check that carries weight.
    local _sums="${_dir}/${_asset}.sha256"
    if try_download "${_base}/${_asset}.sha256" "$_sums"; then
        local _want
        local _got
        _want="$(cut -d' ' -f1 < "$_sums" | tr -d '\r')"
        _got="$(compute_sha256 "$_archive")"
        if [ "$_want" != "$_got" ]; then
            err "checksum mismatch for ${_asset}:
  expected: $_want
  actual:   $_got
the download is not what the release published — do not run it"
        fi
        say "checksum ok"
    else
        warn "no published checksum for ${_asset}"
    fi

    # Provenance ties the archive to the workflow run that built it, which
    # a checksum served from the same place as the binary cannot do.
    # --bundle keeps it offline: no API call, no logged-in gh.
    if check_cmd gh; then
        local _bundle="${_dir}/${_asset}.sigstore.json"
        if try_download "${_base}/${_asset}.sigstore.json" "$_bundle"; then
            if gh attestation verify "$_archive" --bundle "$_bundle" \
                --repo "$REPO" > /dev/null 2>&1; then
                say "provenance verified"
            else
                err "provenance verification failed for ${_asset}.
these bytes are not the ones this repository's release workflow built"
            fi
        else
            warn "no attestation published for ${_asset}"
        fi
    else
        warn "gh not installed, skipping provenance check (checksum only)"
    fi
}

detect_target() {
    local _os
    local _arch
    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Linux)
            # Only glibc builds are published. On musl the binary would
            # load and fail with a linker error that explains nothing.
            if is_musl; then
                err "Alpine/musl is not supported by the published builds.
build from source instead: https://github.com/${REPO}"
            fi
            _os=unknown-linux-gnu
            ;;
        Darwin) _os=apple-darwin ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            err "run install.ps1 on Windows, not install.sh"
            ;;
        *) err "unsupported operating system: $_os" ;;
    esac

    case "$_arch" in
        x86_64|x64|amd64)
            # Under Rosetta `uname -m` reports x86_64 on Apple Silicon, so
            # asking the kernel directly is the only way to avoid handing
            # an arm64 Mac the Intel build.
            if [ "$_os" = apple-darwin ] && is_rosetta; then
                _arch=aarch64
            else
                _arch=x86_64
            fi
            ;;
        arm64|aarch64) _arch=aarch64 ;;
        *) err "unsupported architecture: $_arch" ;;
    esac

    echo "${_arch}-${_os}"
}

is_musl() {
    [ -f /etc/os-release ] && grep -qi alpine /etc/os-release 2>/dev/null
}

is_rosetta() {
    check_cmd sysctl && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]
}

post_install() {
    say "installed agentcanpay to ${BIN_DIR}/agentcanpay"
    case ":${PATH}:" in
        *":${BIN_DIR}:"*) ;;
        *)
            say ""
            say "add it to your PATH:"
            say ""
            say "  export PATH=\"\$PATH:${BIN_DIR}\""
            ;;
    esac
}

compute_sha256() {
    if check_cmd sha256sum; then
        sha256sum "$1" | cut -d' ' -f1
    elif check_cmd shasum; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        err "need sha256sum or shasum to verify the download"
    fi
}

# --fail matters more than it looks: without it curl writes GitHub's 404
# page into the file and the failure surfaces as a corrupt archive.
try_download() {
    if check_cmd curl; then
        curl --proto '=https' --tlsv1.2 --silent --fail --location "$1" \
            --output "$2" 2>/dev/null
    elif check_cmd wget; then
        wget --https-only --secure-protocol=TLSv1_2 -q "$1" -O "$2" 2>/dev/null
    else
        return 1
    fi
}

download() {
    if try_download "$1" "$2"; then
        return 0
    fi
    err "could not download $1
there may be no published build for '$3', or the network is unreachable"
}

downloader() {
    if [ "$1" = --check ]; then
        check_cmd curl || check_cmd wget || err "need curl or wget"
    fi
}

check_cmd() { command -v "$1" > /dev/null 2>&1; }
need_cmd() { check_cmd "$1" || err "need '$1' (command not found)"; }

say() { [ "$QUIET" = yes ] || printf 'agentcanpay: %s\n' "$1" >&2; }
warn() { printf 'agentcanpay: warning: %s\n' "$1" >&2; }
err() { printf 'agentcanpay: %s\n' "$1" >&2; exit 1; }

main "$@"
