Nord Stage 4 Emulator - release archive
=======================================

Open-source software emulator, DSP engine and binary-format toolkit for the
Nord Stage 4 keyboard. Built for interoperability, research and preservation.
Source: https://github.com/ymatrix1/nord-stage-4-emulator

Status: early release
---------------------

The CLAP and VST3 plugin has NOT been verified in any DAW yet. Automated tests
cover the plugin code and the engine, but nobody has loaded this build in
Reaper, Bitwig, Ableton Live or another host and checked audio, MIDI routing,
automation and state recall. Expect rough edges. See HOST_COMPATIBILITY.md in
this archive for the current matrix, and please report what you find.

The builds are unsigned. The archive contains no firmware, factory sounds or
sample data from the hardware; sounds come from the built-in engine and from
files you load yourself.

Contents
--------

  ns4                       command-line tool (inspect presets, offline render)
  nord-stage4               standalone GUI (audio + MIDI)
  nord-plugin-standalone    standalone host of the plugin build
  nord-plugin.clap          CLAP plugin
  nord-plugin.vst3          VST3 plugin (a folder bundle; copy the whole folder)
  HOST_COMPATIBILITY.md     what has and has not been tested
  README.txt                this file

(Windows binaries end in .exe.)

Verify the download
-------------------

Every archive has a .sha256 file on the release page (SHA256SUMS.txt holds all of
them). Put the archive and its .sha256 file in one folder and run:

  Linux:    sha256sum -c <archive>.sha256
  macOS:    shasum -a 256 -c <archive>.sha256
  Windows:  (Get-FileHash <archive> -Algorithm SHA256).Hash
            and compare it with the hash in the .sha256 file.

A checksum detects a damaged download; it is not a signature.

Install the plugin
------------------

Copy the plugin into one of these folders, then rescan plugins in your DAW.

Windows (x86_64)
  CLAP, all users:   C:\Program Files\Common Files\CLAP\nord-plugin.clap
  CLAP, this user:   %LOCALAPPDATA%\Programs\Common\CLAP\nord-plugin.clap
  VST3, all users:   C:\Program Files\Common Files\VST3\nord-plugin.vst3\
  VST3, this user:   %LOCALAPPDATA%\Programs\Common\VST3\nord-plugin.vst3\
  The VST3 folder must keep its layout:
  nord-plugin.vst3\Contents\x86_64-win\nord-plugin.vst3

macOS (Apple silicon, arm64)
  CLAP, all users:   /Library/Audio/Plug-Ins/CLAP/nord-plugin.clap
  CLAP, this user:   ~/Library/Audio/Plug-Ins/CLAP/nord-plugin.clap
  VST3, all users:   /Library/Audio/Plug-Ins/VST3/nord-plugin.vst3
  VST3, this user:   ~/Library/Audio/Plug-Ins/VST3/nord-plugin.vst3
  The builds are not signed or notarized. If macOS refuses to open them, clear
  the quarantine flag on the files you installed, for example:
    xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/nord-plugin.vst3
    xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/CLAP/nord-plugin.clap
  Do the same for ns4, nord-stage4 and nord-plugin-standalone before running them.

Linux (x86_64)
  CLAP, this user:   ~/.clap/nord-plugin.clap
  CLAP, all users:   /usr/lib/clap/nord-plugin.clap
  VST3, this user:   ~/.vst3/nord-plugin.vst3/
  VST3, all users:   /usr/lib/vst3/nord-plugin.vst3/
  The VST3 folder must keep its layout:
  nord-plugin.vst3/Contents/x86_64-linux/nord-plugin.so
  The GUI needs ALSA and X11/OpenGL libraries; the plugin needs a host that
  provides its own window.

Run the standalone programs
---------------------------

  ns4 info
  ns4 render-demo --frames 48000
  nord-stage4

The programs are not installed anywhere; run them from the extracted folder.
