# rlm2c

A mouse (and keyboard) to controller emulator for playing certain games (Fortnite, Apex Legends) When active, all mouse and keyboard input is prevented from reaching Windows, avoiding issues with the game receiving both emulated controller and real button inputs.

This is built using oblitum's [Interception](http://www.oblita.com/interception) driver for gathering and blocking keyboard and mouse input, and nefarious' [ViGEm](https://vigem.org/) for controller emulation.

**WARNING: The interception driver is a potential security threat, and is blocked by certain anti-cheats (FACEIT in particular).**

**WARNING 2: If you are not in posession of moderate computer literacy skills you will most likely fail. This software is provided as-is, please _do not_ message me if you do not know what to do or are unable to get this working.**

# Installation and Usage

1. Head over to the [Releases](https://github.com/hyperxmason/rlm2cedit/releases/tag/new) page, then download and extract the latest archive.
2. Install [Interception](http://www.oblita.com/interception) by running `install-interception/install.bat`.
3. Install the [ViGEm Bus Driver](https://github.com/ViGEm/ViGEmBus/releases).
4. Run `rlm2c.exe` and head over to [Gamepad Tester](https://gamepad-tester.com/) to check things are working as expected. The default key to switch between normal mode where keyboard and mouse input are passed through to Windows and emulation mode where all keyboard and mouse input is fed only to the emulated controller is grave/tilde/the key to the left of 1 and above tab.
