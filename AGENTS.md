
Goal
____

Reproduire le look de gnome-shell sous niri

TOUTE L'INTERFACE EN ANGLAIS.

+ panel du haut avec 
    - à gauche : "Activites", des symboles représetant les workspace, 
    - au milieu : la date
    - à droite : les icones réseau, volume, sesssion
                 ajout versus gnome-shell : notification tray
+ le menu activites est un launcher similair à celui de gnome-shell
+ la date affiche le calendrier

L'expérience doit être très proche de gnome shell.



De plus, dans un jalon ultérieur, un écran de préférence permettra
+ de gérer le wallpaper sous niri
+ de définir un lock screen



techno
______

TypeScript, Astal
La RAM restera légère
- le launcher est détruit après usage
- lockscreen : à la demande
- wallpaper : piloter swaybg
- préférence : mini app isolée


AGENTS
_____

also read AGENTS-local.md


archi
_____



un processus unique (daemon) qui maintient l’état global (workspaces niri, notifications, réseau, audio, config) et pilote plusieurs fenêtres LayerShell


Jalon 1 — Socle & Top Panel
Connexion au socket IPC de niri (EventStream pour les workspaces en temps réel).
Barre supérieure (anchor: top, exclusiveZone activé) avec les 3 zones :
Gauche : bouton Activités + pilules/symboles de workspaces cliquables.
Centre : horloge/date.
Droite : indicateurs système + compteur/icône de notifications.

Jalon 2 — Popups & Services
Popup Calendrier sous la date.
Quick Settings unifié (réseau, volume, session : lock/suspend/reboot/poweroff) façon panneau de contrôle GNOME.
Daemon de notifications (org.freedesktop.Notifications) + tiroir de notifications.

Jalon 3 — Launcher “Activités”
Fenêtre overlay plein écran (avec raccourci clavier ou clic sur Activités).
Barre de recherche + grille d’applications (.desktop + icônes XDG), navigation clavier (Échap, flèches, Entrée).

Jalon 4 — Préférences, Wallpaper & Lock Screen
Fichier de configuration centralisé (JSON/TOML) écouté à chaud par le shell.
Écran de préférences (sélection image, mode d’affichage, options lock screen).
Wallpaper géré en surface layer: background (ou via swaybg/swww piloté par le shell) + Lock screen via ext-session-lock-v1 (avec PAM pour l’authentification).
