# Prima prova su RX 5600 XT

## 1. Preparazione

Usa Windows 10/11 x64 e il driver AMD installato. Chiudi MSI Afterburner e
disattiva la regolazione ventole negli altri tool. Il primo avvio è in sola
lettura: la curva del driver non viene modificata all'apertura dell'app.
Mantieni accessibile Radeon Software per verificare indipendentemente le letture.

## 2. Verifica lettura, senza scrivere

Dal pacchetto portatile, nella sua directory:

```powershell
.\fan-cli.exe list
.\fan-cli.exe probe 'pci:41:0:0'
```

Sostituisci l'ID con quello restituito da `list`. `probe` stampa i cinque punti
correnti e i range, poi tre campioni distanziati di 2 s. Confronta edge/hotspot/RPM
con Radeon Software a riposo. I campioni non sono sincronizzati fra i due tool:
piccole differenze non dimostrano un errore. Sensori assenti sono `null`, non zero.
Se mancano entrambe le temperature, l'applicazione della curva resta bloccata.

Una prova read-only sul PC di sviluppo (2026-10-27) ha dato:

```text
AMD Radeon RX 5600 XT | pci:41:0:0 | Overdrive 8
Curva nativa supportata: sì
Range dichiarati: 25–100 °C, 20–100%
Campioni idle: edge 48–49 °C, hotspot 48–49 °C, 135 RPM, rapporto velocità 4%
```

Sono dati osservati dal driver, non una certificazione di accuratezza dei sensori.
Il fallback PMLog legacy è stato necessario con questo driver. L'uguaglianza
edge/hotspot e il basso RPM devono essere confrontati con una lettura indipendente.

## 3. Prova temporizzata esplicita

Chiudi prima la GUI, copia/modifica `config.example.json` se necessario e avvia:

```powershell
.\fan-cli.exe test-curve 'pci:41:0:0' .\config.example.json
```

Il comando applica una curva non decrescente (30/40/60/80/100%), la rilegge e
campiona per 15 s, poi richiede il ripristino ai default ventola. Fallimenti di
lettura o soglie termiche interrompono la prova e tentano il ripristino.
Non eseguire stress test durante questa prima prova e non terminare forzatamente
il processo: la prova del kill va fatta in un secondo momento dopo il riscontro
della curva nativa. `Ctrl+C` non è attualmente un percorso di uscita gestito.

Per richiedere esplicitamente i default dopo una prova interrotta:

```powershell
.\fan-cli.exe reset 'pci:41:0:0'
```

“Automatico” significa default delle impostazioni fan in ADL. Un errore viene
stampato e non è considerato un successo. Se il driver non risponde, usare i suoi
controlli ufficiali e riavviare il sistema se necessario.

## 4. Prova GUI e lifecycle

1. Apri l'app; attendi almeno tre letture coerenti.
2. Trascina un punto e modifica gli stessi valori da tastiera. Un valore fuori
   range, temperature duplicate o velocità decrescenti devono essere rifiutati.
3. Salva, applica e verifica la curva in Radeon Software, con il suo tuning inattivo.
4. Osserva l'aumento di RPM, poi premi Automatico/default e verifica il ripristino.
5. Applica di nuovo, chiudi la finestra: l'icona tray deve restare e aggiornare il tooltip.
6. Apri dalla tray ed esci dal relativo menu: controlla il ritorno ai default.
7. Verifica login autostart e avvio nella tray. Le due opzioni sono indipendenti;
   l'avvio nella tray va salvato con Salva impostazioni. Non viene riattivata automaticamente la curva.
8. Solo dopo queste prove valuta sospensione/ripresa, reset driver e processo
   terminato forzatamente, verificando cosa rimane effettivamente nel driver.

Registrare versione del driver, ID GPU, curva, orario e comportamento osservato
quando si apre una segnalazione. I test CI non hanno una GPU AMD: verificano
compilazione, contratti Rust e packaging, non il funzionamento fisico delle ventole.

## 5. Build e release

Il job GitHub Windows installa la toolchain, esegue test/lint e produce MSI,
NSIS EXE e ZIP con GUI+CLI. L'installer usa il bootstrapper WebView2; lo ZIP non
include un runtime browser da centinaia di MB e usa quello installato nel sistema.
I checksum `SHA256SUMS.txt` sono allegati alla release.

Le misure di memoria e CPU vanno fatte sul binario release, includendo tutti i
processi WebView2 associati. Riportare separatamente app visibile e tray nascosta;
non è stato ancora fissato un budget numerico validato. Per ridurre ulteriormente
la memoria sarà possibile separare un daemon senza webview, se le misure lo richiedono.
