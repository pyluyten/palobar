TODO
____

## about top bar

* Top Panel
    - [x] notification tray
        - notification stacking
    - [x] media player
    - [x] workspace button
    - [x] clipboard+
    - [ ] battery
    - [ ] power profile
    - [x] dark, light
    - [ ] night mode
    - [ ] vpn
    - [x] screenshot (region)
    - [ ] bluetooth
* Launcher
    - [x] text entry
    - [x] dock
        + add a command to toggle dock
    - [x] pages
* [x] Command launcher
    - command launcher history dialog
    - more?


## about settings

`env XDG_CURRENT_DESKTOP=GNOME gnome-control-center` to run it and use it or compare.

| gnome-settings    | palobar                                      |
| ----------------- | -------------------------------------------- |
| wifi              | todo. but quicksettings already provide 99%. |
| network           | todo. but minor                              |
| bluetooth         | **todo**                                     |
| screens           | ☑ *Done* for v1.                             |
| sound             | ☑ *Done*.                                    |
| energy            | todo                                         |
| multi task        | ☑ Done, in a sense...                        |
| appearance        | ☑ Done                                       |
| notification      | todo                                         |
| research          | could it be?                                 |
| accounts          | todo , or not                                |
| share             | todo                                         |
| wellbeing         | todo                                         |
| mouse             | ☑ Done for v1.                               |
| keyb              | ☑ Done for v1.                               |
| colors            | todo ?                                       |
| printers          | ☑ *DONE*                                     |
| accessibilty      | todo                                         |
| private -lock     | todo                                         |
| private -gps      | todo                                         |
| private -history  | todo                                         |
| private -camera   | todo                                         |
| private -security | todo                                         |
| system-locale     | todo                                         |
| system-datetime   | todo                                         |
| system-datetime   | todo                                         |
| system-users      | todo                                         |
| system-ssh        | todo                                         |
| system-about      | ☑ Done                                       |

More settings from "tweaks"

| gnome-adjustment | palobar                                      |
| ---------------- | -------------------------------------------- |
| font             | todo. but quicksettings already provide 99%. |
| rendering        | todo. but minor                              |
| cursor           | ☑ Done (in mouse)                            |
| icons            |                                              |
| startup apps     | *Parse Niri? add to Niri ?*                  |

## Meta

- [x] Change tech if too heavy or uneasy
  gjs/astal was fine for dev & usage (slighly high ram), but
  dependency is not suitable.
  rewrote to pure rust gtk.

- [ ] Be easy to install
  appimage proved feasible to solve the issue when ags/astal are not available.
  more testing need but "easy to install" may be solved


- [ ] Plugins
  would be nice to be able to activate or not some applets
  drop some custom css


