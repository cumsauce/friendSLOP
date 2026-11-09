# Paste into hosts/server/configuration.nix (next to the `rot` container), or import this file.
# Add `idea_bot_discord_token` and `idea_bot_github_token` to secrets/secrets.yaml with sops.
{ config, ... }:
{
  virtualisation.oci-containers.containers."friendslop-idea-bot" = {
    image = "ghcr.io/cumsauce/friendslop-idea-bot:latest";
    pull = "newer";
    environmentFiles = [ config.sops.templates."friendslop-idea-bot.env".path ];
    environment = {
      GITHUB_REPO = "cumsauce/friendSLOP";
      ANNOUNCE_CHANNEL_ID = "000000000000000000"; # your #friendslop channel id
      STATE_PATH = "/data/state.json";
      REMINDER_TZ = "UTC"; # noon in this zone re-asks unanswered questions (#157)
      AVATAR_HOURS = "3"; # redraw the bot's profile picture from the game status (#170)
    };
    volumes = [ "friendslop-idea-bot:/data" ];
  };

  sops.secrets."idea_bot_discord_token" = { };
  sops.secrets."idea_bot_github_token" = { };
  sops.templates."friendslop-idea-bot.env".content = ''
    DISCORD_TOKEN=${config.sops.placeholder."idea_bot_discord_token"}
    GITHUB_TOKEN=${config.sops.placeholder."idea_bot_github_token"}
  '';
}
