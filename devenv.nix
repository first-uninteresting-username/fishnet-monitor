{pkgs, ...}: let
  # Static binary for the container image (see PLAN.md, "Technology").
  muslTarget = "${pkgs.stdenv.hostPlatform.parsed.cpu.name}-unknown-linux-musl";
in {
  packages = [
    pkgs.git
    pkgs.cargo-nextest
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src"];
    targets = [muslTarget];
  };

  env.MUSL_TARGET = muslTarget;

  scripts = {
    build-static.exec = ''cargo build --release --target "$MUSL_TARGET" "$@"'';
    demo.exec = ''cargo run -- demo "$@"'';
  };

  # Run by `devenv test` (also in the update-devenv-lock workflow).
  enterTest = ''
    cargo --version
    if [ -f Cargo.toml ]; then
      cargo fmt --check
      cargo clippy --all-targets -- -D warnings
      cargo nextest run --no-tests=pass
    fi
  '';

  git-hooks.hooks = {
    check-added-large-files.enable = true;
    check-merge-conflicts.enable = true;
    detect-private-keys.enable = true;
    end-of-file-fixer = {
      enable = true;
      excludes = ["devenv.lock"];
    };
    trim-trailing-whitespace.enable = true;
    check-json.enable = true;
    check-yaml.enable = true;
    alejandra.enable = true;

    rustfmt.enable = true;
    clippy = {
      enable = true;
      settings.denyWarnings = true;
    };
    actionlint.enable = true;
    hadolint.enable = true;
  };
}
