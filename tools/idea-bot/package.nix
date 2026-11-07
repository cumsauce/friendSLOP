{ python3Packages, lib }:

python3Packages.buildPythonApplication {
  pname = "friendslop-idea-bot";
  version = "0.1.0";
  pyproject = false;
  src = ./.;
  dependencies = with python3Packages; [
    discordpy
    aiohttp
  ];
  installPhase = ''
    install -Dm755 bot.py $out/bin/friendslop-idea-bot
  '';
  meta.mainProgram = "friendslop-idea-bot";
}
