# FishSteam

FishSteam is a thing allowing to link with https://docs.rs/steamworks-rs without Steam SDK installed on a builder machine. Helpful for building a game with steamworks on GitHub Actions.

There is no steam confidential data here, only two libraries: one use "steamworks-rs" as a dependency, and another use "dlopen" to communicate with the first library at runtime.

Alternative solutions:
- Build on local developers machines only. It is actually not that bad, until you need to deal with MacOs. GitHub Actions helps a ton when you do not have a physical mac but still need to build things.
- Use a private CI. Either private GitHub actions container or just a private CI setup. Put all questionably licensed things into a private container. It is the easiest way, but may be a bit pricey.

# How to use FishSteam

1. Build a dll for each platform once.

```bash
> cd fishsteam/fishsteam-sys

> env STEAM_SDK_LOCATION=/path/to/sdk cargo build --release
```

2. Set up a CI to build a game

Use `fishsteam` as a dependency. It do not require STEAM_SDK_LOCATION and will build just fine.
But each steam call will panic without a dynamic library sitting next to a game binary.

3. Grab artifacts for each platform from the CI, put a compiled libraries next to them and do all the steam upload machinery.

This may be automated as a CI step, thanks to

https://hub.docker.com/r/cm2network/steamcmd
https://hub.docker.com/r/cm2network/steampipe

# Using FishSteam for other games

Well, "fishsteam" is really mostly about the fish fight game. But it is super small and simple, if you need different features - just copy-paste everything and modify for your game. And lets hope it will all got a better solution and it will be possible to just use the SDK on a public CI!

# This seems *fishy*, you sure there is no better way?

I really hope there is. I would **love** to nuke this repo and use someone's else work.
