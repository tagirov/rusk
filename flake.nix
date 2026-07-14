{
  # Краткое описание флейка — видно в выводе `nix flake show` и `nix flake metadata`
  description = "rusk — minimal cross-platform terminal task manager";

  # inputs — внешние зависимости флейка.
  # Nix скачает их и зафиксирует точные ревизии в flake.lock,
  # поэтому сборка воспроизводима: у всех одинаковый nixpkgs.
  inputs = {
    # nixpkgs-unstable нужен из-за edition 2024 — требуется rustc >= 1.85
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  # outputs — функция, которая из inputs строит всё, что флейк предоставляет:
  # пакеты, dev-окружения, приложения и т.д.
  outputs =
    { self, nixpkgs }:
    let
      # Список систем, для которых объявляем выходы
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      # Хелпер: применяет функцию f к каждой системе и собирает
      # атрибут-сет вида { x86_64-linux = ...; aarch64-linux = ...; ... }
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # `nix build` / `nix profile install` берут пакеты отсюда
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "rusk";
          # Версию читаем прямо из Cargo.toml, чтобы не дублировать её руками
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          # Исходники — сам репозиторий флейка.
          # cleanSource отбрасывает мусор вроде target/ и .git,
          # чтобы правки там не вызывали пересборку
          src = pkgs.lib.cleanSource ./.;

          # Вместо хеша вендоренных зависимостей используем Cargo.lock:
          # Nix сам скачает все крейты по зафиксированным в нём версиям
          cargoLock.lockFile = ./Cargo.lock;

          # installShellCompletion кладёт готовые файлы автодополнения
          # в правильные места внутри $out
          nativeBuildInputs = [ pkgs.installShellFiles ];

          # postInstall выполняется после `cargo install`:
          # докладываем комплиты рядом с бинарником
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
            mainProgram = "rusk"; # что запускать при `nix run`
          };
        };
      });

      # `nix develop` — dev-окружение с тулчейном для работы над проектом
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
