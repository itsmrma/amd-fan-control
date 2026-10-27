Prima alpha Windows x64 per AMD Radeon RX 5600 XT.

- Rust + Tauri v2, frontend TypeScript vanilla con grafico SVG interattivo.
- Lettura temperatura edge/junction, RPM e rapporto velocità ventola tramite ADL.
- Applicazione e rilettura di curve native Overdrive8 a cinque punti.
- Ripristino delle sole impostazioni ventola ai default del driver, tray e autostart.
- MSI, installer NSIS, ZIP portatile con CLI diagnostica e checksum SHA-256.
- Logo SVG originale, icone app e supporto tema di sistema.

La lettura hardware e le capability OD8 sono state verificate su una RX 5600 XT.
Le scritture ventola richiedono ancora la prova hardware descritta in TESTING.md:
questa è un'alpha, non una release con affidabilità già certificata.

Avviare una sola utility di tuning alla volta. All'avvio l'app è in monitoraggio;
la curva personalizzata richiede un'applicazione esplicita. La chiusura della
finestra minimizza nella tray; “Esci” tenta il ripristino automatico prima di
terminare. In caso di kill forzato resta la curva nativa nel driver: il ritorno
immediato ai default BIOS non è garantito e non è implementato un watchdog esterno.

L'isteresi software e punti in numero variabile sono predisposti nel core, ma non
esposti dal backend OD8 di questa alpha. Linux ha il trait, non ancora un backend
sysfs. Il pacchetto portatile richiede Microsoft Edge WebView2 Runtime installato;
gli installer scaricano il bootstrapper quando necessario. Binari non firmati.
