{
  # Short flake description, shown by `nix flake show` and `nix flake metadata`
  description = "rusk — minimal cross-platform terminal task manager";

  # inputs — external dependencies of the flake.
  # Nix fetches them and pins exact revisions in flake.lock,
  # so the build is reproducible: everyone gets the same nixpkgs.
  inputs = {
    # nixpkgs-unstable is required because of edition 2024 (needs rustc >= 1.85)
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  # outputs — a function that builds everything the flake provides from inputs:
  # packages, dev environments, apps, etc.
  outputs =
    { self, nixpkgs }:
    let
      # Systems we declare outputs for
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      # Helper: applies f to every system and collects an attribute set
      # of the form { x86_64-linux = ...; aarch64-linux = ...; ... }
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # `nix build` / `nix profile install` take packages from here
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "rusk";
          # Read the version straight from Cargo.toml to avoid duplicating it by hand
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          # Sources are the flake repository itself.
          # cleanSource drops junk like target/ and .git,
          # so changes there do not trigger a rebuild
          src = pkgs.lib.cleanSource ./.;

          # Use Cargo.lock instead of a vendored dependencies hash:
          # Nix fetches all crates at the versions pinned there
          cargoLock.lockFile = ./Cargo.lock;

          # installShellCompletion places the prebuilt completion files
          # into the right locations inside $out
          nativeBuildInputs = [ pkgs.installShellFiles ];

          # Git backend tests call the git binary, which is absent in the build sandbox
          nativeCheckInputs = [ pkgs.git ];

          # postInstall runs after `cargo install`:
          # ship the completions next to the binary
          postInstall = ''
            installShellCompletion \
              --bash completions/rusk.bash \
              --zsh completions/rusk.zsh \
              --fish completions/rusk.fish
          '';

          meta = {
            description = "Minimal cross-platform terminal task manager";
            homepage = "https://github.com/tagirov/rusk";
            license = pkgs.lib.licenses.gpl3Only;
            mainProgram = "rusk"; # what `nix run` executes
          };
        };
      });

      # `nix develop` — dev environment with the toolchain for working on the project
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
          ];
        };
      });
    };
}
