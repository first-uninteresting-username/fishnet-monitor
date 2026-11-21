{pkgs, ...}: let
  # Static binary for the container image (see PLAN.md, "Technology").
  muslTarget = "${pkgs.stdenv.hostPlatform.parsed.cpu.name}-unknown-linux-musl";
in {
  packages = [
    pkgs.git
    pkgs.cargo-nextest
    pkgs.curl # scripts/vendor-frontend.sh
    pkgs.miniserve # demo-ui
    pkgs.python3 # scripts/mock-fishnet.py (smoke-test)
    pkgs.tailwindcss_4 # build-css: frontend/tailwind.css -> frontend/styles.css
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
    # The dashboard with mock data, no backend needed: http://localhost:8080/demo/
    demo-ui.exec = ''miniserve --port 8080 --index index.html "$DEVENV_ROOT" "$@"'';
    vendor-frontend.exec = ''"$DEVENV_ROOT/scripts/vendor-frontend.sh"'';
    # Tailwind: compiles frontend/tailwind.css into frontend/styles.css (committed,
    # so the Rust and Docker builds need no Tailwind). `watch-css` rebuilds on save.
    build-css.exec = ''tailwindcss --cwd "$DEVENV_ROOT/frontend" -i tailwind.css -o styles.css --minify "$@"'';
    watch-css.exec = ''build-css --watch "$@"'';
    # The CI smoke test against the host's podman (not part of devenv). The
    # docker format keeps the HEALTHCHECK, which OCI images drop.
    smoke-test.exec = ''
      podman build --format docker -t localhost/fishnet-monitor:smoke "$DEVENV_ROOT" &&
        CONTAINER=podman "$DEVENV_ROOT/scripts/smoke-test.sh" localhost/fishnet-monitor:smoke "$@"
    '';
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

  # Vendored files must stay byte-identical to frontend/vendor.lock.
  git-hooks.excludes = ["^frontend/vendor/"];

  git-hooks.hooks = {
    check-added-large-files.enable = true;
    check-merge-conflicts.enable = true;
    detect-private-keys.enable = true;
    end-of-file-fixer = {
      enable = true;
      excludes = ["devenv.lock" "^frontend/styles\\.css$"];
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

    # frontend/styles.css is generated: rebuild it whenever its sources change.
    build-css = {
      enable = true;
      name = "build-css";
      entry = "${pkgs.tailwindcss_4}/bin/tailwindcss --cwd frontend -i tailwind.css -o styles.css --minify";
      files = "^(frontend/(tailwind\\.css|src/.*\\.js|vendor/uplot\\.css)|demo/.*\\.(js|html))$";
      pass_filenames = false;
    };
  };
}
