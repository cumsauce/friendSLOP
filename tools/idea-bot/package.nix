{ python3Packages, lib }:

python3Packages.buildPythonApplication {
  pname = "friendslop-idea-bot";
  version = "0.1.0";
  pyproject = false;
  src = ./.;
  dependencies = with python3Packages; [
    discordpy
    aiohttp
    tzdata
    # the profile picture renderer (avatar.py, #170)
    numpy
    pillow
  ];
  installPhase = ''
    install -Dm755 bot.py $out/bin/friendslop-idea-bot
    install -Dm644 avatar.py $out/${python3Packages.python.sitePackages}/avatar.py
  '';
  meta.mainProgram = "friendslop-idea-bot";
}
