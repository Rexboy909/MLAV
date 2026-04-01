# MLAV
### Music and Lyric Audio Visualizer
##### MLAV will be a music player that dynamically displays the lyrics of the song playing and features visualizers for the songs.
##### The program will support uncompressed audio, like .wav and .flac files, as well as other formats, like mp3, aac, and ogg.
##### The application will render different audio visualizers for the current song and will try to incorporate lyrics into the visualizer.

## Year two Concepts (currently)
1. Packages
-   Rust has Modules, which are much more versatile versions of classes and hierarchy
2. GUI
-   A running window, currently no interactability, only display.
3. File I/O
4. Exception handling
-  The goal is that if, for some reason, something doesnt work or loads incorrectly, the program can adapt and not crash.

## Current Features:
1. Sound output
2. Song Selection and Navigation
3. Several sound formats supported
4. A simple 2d visualizer
5. a 3d Visualiser is being worked on, not yet completed

## Compile instructions (Executables will be provided at a later date):
#### 1: Make sure you have [Rust](https://rust-lang.org/tools/install/) installed.
- Go to the site
- run Install script (if on unix os or MacOS) or run installer (if on windows)
#### 2: Download source code and navigate to [MLAV/] folder in a terminal.
#### 3: run "cargo run" in the terminal. Rust, using Cargo, should handle compilation and running.

##### GUI mockup
![Image of running app](https://github.com/Rexboy909/MLAV/blob/main/docs/mockups-02.png?raw=true)
[Source](docs/)

##### Running app as of 2/23/26
![Image of running app](https://github.com/Rexboy909/MLAV/blob/main/docs/Running_app_2-23-26.png?raw=true)
[Source](docs/)
###### very barebones

##### Rough UML Diagram
![Image of running app](https://github.com/Rexboy909/MLAV/blob/main/docs/UML_DIAG.png?raw=true)
[Source](docs/)
