#!/usr/bin/env -S uv run --quiet --with openpyxl --with python-docx --with reportlab python
"""Builds a test folder that behaves like a real small company's shared drive.

Written by hand rather than scraped: a real firm's documents would carry real
people's names and prices, and none of what this folder has to exercise —
a discount stated twice with different numbers, a price list kept beside last
year's, a scan with no text layer — depends on the content being genuine.

What it deliberately contains, because these are the cases that break things:

  * the same claim in two documents with two different numbers (5% vs 8%)
  * a response time contradicted between a contract and a note (24 h vs 48 h)
  * last year's price list sitting next to this year's
  * a byte-for-byte duplicate under a different name
  * a PDF that is a scan, with no text layer at all
  * a payroll spreadsheet that must be held back
  * Word lock files, .DS_Store, a CAD drawing and a logo
  * Windows-1250 encoding on one file, CRLF on another
"""

import os
import shutil
import time
from datetime import datetime
from pathlib import Path

from docx import Document
from docx.shared import Pt
from openpyxl import Workbook
from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

ROOT = Path(__file__).parent / "termoval" / "source" / "Termoval - Prodaja"


def write(rel: str, text: str, *, encoding: str = "utf-8", newline: str = "\n") -> None:
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    body = text.replace("\n", newline) if newline != "\n" else text
    path.write_bytes(body.encode(encoding, errors="replace"))


def docx(rel: str, blocks: list[tuple[str, str]]) -> None:
    """blocks: (style, text) where style is 'h1', 'h2' or 'p'."""
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    doc = Document()
    doc.styles["Normal"].font.size = Pt(11)
    for style, text in blocks:
        if style == "h1":
            doc.add_heading(text, level=1)
        elif style == "h2":
            doc.add_heading(text, level=2)
        else:
            doc.add_paragraph(text)
    doc.save(path)


def xlsx(rel: str, sheets: dict[str, list[list]]) -> None:
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    wb = Workbook()
    wb.remove(wb.active)
    for name, rows in sheets.items():
        ws = wb.create_sheet(title=name)
        for row in rows:
            ws.append(row)
    wb.save(path)


def pdf_with_text(rel: str, lines: list[str]) -> None:
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    c = canvas.Canvas(str(path), pagesize=A4)
    _, height = A4
    y = height - 70
    for line in lines:
        if line == "---page---":
            c.showPage()
            y = height - 70
            continue
        c.setFont("Helvetica-Bold" if line.startswith("#") else "Helvetica", 13 if line.startswith("#") else 10.5)
        # reportlab's built-in fonts are Latin-1; the diacritics survive.
        c.drawString(60, y, line.lstrip("# "))
        y -= 22 if line.startswith("#") else 16
    c.save()


def pdf_without_text(rel: str) -> None:
    """A scan: ink on the page, no text layer. Extraction must refuse it."""
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    c = canvas.Canvas(str(path), pagesize=A4)
    width, height = A4
    c.setLineWidth(0.6)
    for i in range(26):
        y = height - 90 - i * 24
        c.line(70, y, 70 + (380 if i % 3 else 210), y)
    c.rect(70, 120, 200, 90)
    c.save()


# ---------------------------------------------------------------------------

if ROOT.exists():
    shutil.rmtree(ROOT)
ROOT.mkdir(parents=True)

# -- 01 Uvjeti i pravila ----------------------------------------------------

docx(
    "01 Uvjeti i pravila/Opci-uvjeti-poslovanja-2026.docx",
    [
        ("h1", "Opći uvjeti poslovanja"),
        ("p", "Termoval d.o.o., Zagreb, OIB 12345678901. Vrijede od 1. siječnja 2026."),
        ("h2", "Cijene"),
        ("p", "Sve cijene u ovom dokumentu iskazane su bez PDV-a."),
        ("p", "Cijene vrijede za isporuku na području Grada Zagreba i Zagrebačke županije. Za isporuke izvan tog područja zaračunava se prijevoz prema važećem cjeniku."),
        ("h2", "Popusti"),
        ("p", "Popust od 5% odobrava se stalnim kupcima, odnosno kupcima koji su u prethodnih dvanaest mjeseci ostvarili promet veći od 20.000,00 EUR."),
        ("p", "Popust veći od 10% odobrava isključivo voditelj prodaje, uz pisanu suglasnost."),
        ("h2", "Ponude"),
        ("p", "Ponuda vrijedi 14 dana od datuma izdavanja, osim ako je u samoj ponudi navedeno drugačije."),
        ("p", "Broj krugova izmjena ponude ograničen je na dva. Svaka daljnja izmjena naplaćuje se prema satnici projektanta."),
        ("h2", "Plaćanje"),
        ("p", "Rok plaćanja je 15 dana od dana izdavanja računa."),
        ("p", "Za radove čija vrijednost prelazi 5.000,00 EUR naplaćuje se avans u iznosu od 40% ugovorene vrijednosti."),
        ("h2", "Jamstvo"),
        ("p", "Jamstveni rok na ugradnju iznosi 24 mjeseca od dana primopredaje."),
        ("p", "Jamstvo ne pokriva kvarove nastale nepravilnim rukovanjem niti izostankom redovnog godišnjeg servisa."),
        ("h2", "Servis"),
        ("p", "Vrijeme izlaska na teren po prijavi kvara iznosi 24 sata za ugovorne korisnike."),
    ],
)

# Last year's version, left in the same folder. Says 8% where the current
# document says 5% — this is the conflict the review screen has to catch.
docx(
    "01 Uvjeti i pravila/Opci-uvjeti-poslovanja-2025-STARO.docx",
    [
        ("h1", "Opći uvjeti poslovanja"),
        ("p", "Termoval d.o.o., Zagreb. Vrijede od 1. siječnja 2025."),
        ("h2", "Popusti"),
        ("p", "Popust od 8% odobrava se stalnim kupcima, odnosno kupcima koji su u prethodnih dvanaest mjeseci ostvarili promet veći od 15.000,00 EUR."),
        ("h2", "Plaćanje"),
        ("p", "Rok plaćanja je 30 dana od dana izdavanja računa."),
        ("h2", "Jamstvo"),
        ("p", "Jamstveni rok na ugradnju iznosi 12 mjeseci od dana primopredaje."),
    ],
)

write(
    "01 Uvjeti i pravila/Pravilnik-o-popustima.md",
    """# Pravilnik o odobravanju popusta

Interni dokument. Vrijedi od 1. siječnja 2026.

## 1. Redovni popust

Popust od 5% odobrava voditelj prodaje samostalno, bez dodatne suglasnosti.

Uvjet je da je kupac u prethodnih dvanaest mjeseci ostvario promet veći od
20.000,00 EUR bez PDV-a.

## 2. Povećani popust

Popust do 10% odobrava se uz pisanu suglasnost direktora.

Popust veći od 10% ne odobrava se ni u kojem slučaju, osim kod javne nabave
gdje vrijede uvjeti iz natječajne dokumentacije.

## 3. Postupak

1. Prodavač provjerava promet kupca u prethodnih 12 mjeseci.
2. Ako je promet ispod praga, popust se ne odobrava.
3. Ako je traženi popust veći od 5%, zahtjev ide direktoru.
4. Odobreni popust unosi se u ponudu prije slanja.

## 4. Evidencija

Svaki odobreni popust veći od 5% evidentira se u tablici `Popusti-2026.xlsx`
uz ime odobravatelja i datum.
""",
)

pdf_with_text(
    "01 Uvjeti i pravila/Jamstveni-uvjeti.pdf",
    [
        "# Jamstveni uvjeti",
        "Termoval d.o.o. - vrijedi od 1. sijecnja 2026.",
        "",
        "1. Jamstveni rok na ugradnju iznosi 24 mjeseca od dana primopredaje.",
        "2. Jamstveni rok na opremu odreduje proizvodac opreme i naveden je",
        "   u jamstvenom listu koji se predaje kupcu.",
        "3. Jamstvo vrijedi uz uvjet redovnog godisnjeg servisa koji obavlja",
        "   ovlasteni servis Termoval d.o.o.",
        "",
        "---page---",
        "# Sto jamstvo ne pokriva",
        "",
        "- kvarove nastale nepravilnim rukovanjem",
        "- kvarove nastale izostankom redovnog godisnjeg servisa",
        "- ostecenja nastala radovima trecih osoba na instalaciji",
        "- potrosni materijal, filtere i sredstva za ciscenje",
        "",
        "Reklamacija se podnosi pisanim putem u roku od 8 dana od uocavanja",
        "nedostatka, na adresu sjedista drustva.",
    ],
)

# -- 02 Cjenici -------------------------------------------------------------

xlsx(
    "02 Cjenici/Cjenik-2026.xlsx",
    {
        "Klima": [
            ["Šifra", "Stavka", "Jedinica", "Cijena (EUR)"],
            ["KL-001", "Daikin FTXM25R, split 2,5 kW", "kom", 812.00],
            ["KL-002", "Daikin FTXM35R, split 3,5 kW", "kom", 892.00],
            ["KL-003", "Daikin FTXM50R, split 5,0 kW", "kom", 1120.00],
            ["KL-010", "Mitsubishi MSZ-AP25VGK, split 2,5 kW", "kom", 765.00],
            ["KL-011", "Mitsubishi MSZ-AP35VGK, split 3,5 kW", "kom", 848.00],
            ["KL-020", "Multi split vanjska jedinica 2x", "kom", 1340.00],
            ["KL-021", "Multi split vanjska jedinica 3x", "kom", 1795.00],
            ["KL-030", "Kanalska jedinica 7,1 kW", "kom", 2240.00],
        ],
        "Usluge": [
            ["Šifra", "Stavka", "Jedinica", "Cijena (EUR)"],
            ["US-001", "Montaža split sustava do 3,5 kW", "kom", 210.00],
            ["US-002", "Montaža split sustava 3,5–5,0 kW", "kom", 265.00],
            ["US-003", "Montaža multi split sustava, po unutarnjoj jedinici", "kom", 180.00],
            ["US-010", "Bakrena instalacija, isporuka i montaža", "m", 18.50],
            ["US-011", "Probijanje zida do 30 cm", "kom", 35.00],
            ["US-020", "Godišnji servis split sustava", "kom", 65.00],
            ["US-021", "Izlazak na teren, Zagreb", "kom", 40.00],
            ["US-022", "Izlazak na teren, izvan Zagreba", "km", 0.65],
            ["US-030", "Punjenje plina R32", "kg", 42.00],
        ],
    },
)

# Kept beside the current one, as it always is.
xlsx(
    "02 Cjenici/Cjenik-2025-v2.xlsx",
    {
        "Klima": [
            ["Šifra", "Stavka", "Jedinica", "Cijena (EUR)"],
            ["KL-001", "Daikin FTXM25R, split 2,5 kW", "kom", 780.00],
            ["KL-002", "Daikin FTXM35R, split 3,5 kW", "kom", 856.00],
        ],
        "Usluge": [
            ["Šifra", "Stavka", "Jedinica", "Cijena (EUR)"],
            ["US-001", "Montaža split sustava do 3,5 kW", "kom", 195.00],
            ["US-020", "Godišnji servis split sustava", "kom", 58.00],
        ],
    },
)

# Semicolon-delimited with decimal commas, the way a Croatian locale exports.
write(
    "02 Cjenici/Cjenik-dobavljaci.csv",
    """Dobavljač;Šifra;Stavka;Nabavna cijena;Rabat
Klima Centar d.o.o.;KL-001;Daikin FTXM25R;612,00;18%
Klima Centar d.o.o.;KL-002;Daikin FTXM35R;671,00;18%
Termo Oprema d.o.o.;KL-010;Mitsubishi MSZ-AP25VGK;590,00;22%
Termo Oprema d.o.o.;KL-011;Mitsubishi MSZ-AP35VGK;655,00;22%
Instalater d.o.o.;US-010;Bakrena cijev 1/4";9,20;12%
Instalater d.o.o.;US-030;Plin R32, boca 10 kg;280,00;15%
""",
)

# -- 03 Ponude --------------------------------------------------------------

docx(
    "03 Ponude/Ponuda-2026-0142-Hotel-Adriatic.docx",
    [
        ("h1", "Ponuda 2026-0142"),
        ("p", "Kupac: Hotel Adriatic d.d., Opatija"),
        ("p", "Datum: 12. siječnja 2026."),
        ("h2", "Predmet ponude"),
        ("p", "Isporuka i montaža 14 split sustava u sobama drugog kata."),
        ("h2", "Stavke"),
        ("p", "14 × Daikin FTXM35R, split 3,5 kW — 892,00 EUR/kom"),
        ("p", "14 × Montaža split sustava do 3,5 kW — 210,00 EUR/kom"),
        ("p", "180 m × Bakrena instalacija, isporuka i montaža — 18,50 EUR/m"),
        ("h2", "Uvjeti"),
        ("p", "Sve cijene su bez PDV-a."),
        ("p", "Odobren popust: 5% na opremu, temeljem ostvarenog prometa u 2025. godini."),
        ("p", "Ponuda vrijedi 14 dana od datuma izdavanja."),
        ("p", "Rok plaćanja je 15 dana od dana izdavanja računa, uz avans od 40%."),
        ("p", "Jamstveni rok na ugradnju iznosi 24 mjeseca od dana primopredaje."),
    ],
)

docx(
    "03 Ponude/Ponuda-2026-0143-Pekara-Klas.docx",
    [
        ("h1", "Ponuda 2026-0143"),
        ("p", "Kupac: Pekara Klas d.o.o., Velika Gorica"),
        ("p", "Datum: 19. siječnja 2026."),
        ("h2", "Predmet ponude"),
        ("p", "Isporuka i montaža kanalske jedinice 7,1 kW u proizvodnom pogonu."),
        ("h2", "Stavke"),
        ("p", "1 × Kanalska jedinica 7,1 kW — 2.240,00 EUR"),
        ("p", "1 × Montaža multi split sustava, po unutarnjoj jedinici — 180,00 EUR"),
        ("h2", "Uvjeti"),
        ("p", "Sve cijene su bez PDV-a."),
        ("p", "Popust nije odobren; kupac nije ostvario prag prometa."),
        ("p", "Ponuda vrijedi 14 dana od datuma izdavanja."),
    ],
)

docx(
    "03 Ponude/Predlozak-ponude.docx",
    [
        ("h1", "Ponuda [BROJ]"),
        ("p", "Kupac: [NAZIV KUPCA]"),
        ("p", "Datum: [DATUM]"),
        ("h2", "Predmet ponude"),
        ("p", "[OPIS]"),
        ("h2", "Uvjeti"),
        ("p", "Sve cijene su bez PDV-a."),
        ("p", "Ponuda vrijedi 14 dana od datuma izdavanja."),
        ("p", "Rok plaćanja je 15 dana od dana izdavanja računa."),
    ],
)

# -- 04 Servis --------------------------------------------------------------

write(
    "04 Servis/Servisni-postupak.md",
    """# Servisni postupak

## 1. Prijava kvara

Kupac prijavljuje kvar telefonom ili e-poštom na servis@termoval.hr.

Prijava se evidentira u servisni nalog istog radnog dana.

## 2. Izlazak na teren

Vrijeme izlaska na teren po prijavi kvara iznosi 24 sata za ugovorne
korisnike i 72 sata za ostale.

Izlazak se naplaćuje prema cjeniku, osim u jamstvenom roku.

## 3. Servisni nalog

Serviser ispunjava servisni nalog na terenu i evidentira:

- opis kvara kako ga je opisao kupac
- utvrđeni uzrok
- izvršene radove
- utrošeni materijal
- vrijeme na terenu

## 4. Primopredaja

Nakon završetka radova kupac potpisuje primopredajni zapisnik.

Primopredajni zapisnik je dokument kojim kupac potvrđuje da su radovi
izvršeni i kojim počinje teći jamstveni rok.

## 5. Naplata

Račun se izdaje u roku od tri radna dana od potpisa zapisnika.
""",
)

# CRLF and Windows-1250, the way a note typed on an office PC arrives.
write(
    "04 Servis/Vrijeme-izlaska-na-teren.txt",
    """Biljeska sa sastanka servisa, 8. sijecnja 2026.

Dogovoreno je da vrijeme izlaska na teren za ugovorne korisnike bude
48 sati, a ne 24 sata kako pise u opcim uvjetima. Razlog je manjak
servisera u sijecnju i veljaci.

Treba azurirati opce uvjete. Nije jos napravljeno.

Prisutni: Ana Kovac, Marko Juric, Ivan Peric
""",
    encoding="windows-1250",
    newline="\r\n",
)

docx(
    "04 Servis/Primopredajni-zapisnik.docx",
    [
        ("h1", "Primopredajni zapisnik"),
        ("p", "Broj: ____ / 2026"),
        ("p", "Datum primopredaje: ____________"),
        ("h2", "Ugovorne strane"),
        ("p", "Izvođač: Termoval d.o.o., Zagreb"),
        ("p", "Naručitelj: ____________"),
        ("h2", "Izjava"),
        ("p", "Primopredajni zapisnik je dokument kojim naručitelj potvrđuje da su radovi izvršeni prema ponudi i kojim počinje teći jamstveni rok."),
        ("p", "Naručitelj izjavljuje da je pregledao izvedene radove i da nema primjedbi."),
        ("h2", "Potpisi"),
        ("p", "Za izvođača: ____________    Za naručitelja: ____________"),
    ],
)

# -- 05 Interno -------------------------------------------------------------

# Payroll: the file the product has to hold back rather than read.
xlsx(
    "05 Interno/Zaposlenici-place-2026.xlsx",
    {
        "Place": [
            ["Ime i prezime", "OIB", "Radno mjesto", "Bruto plaća", "IBAN"],
            ["Ana Kovač", "98765432109", "Voditelj prodaje", 2450.00, "HR1723600001101234565"],
            ["Marko Jurić", "87654321098", "Serviser", 1680.00, "HR1723600001101234566"],
            ["Ivan Perić", "76543210987", "Serviser", 1620.00, "HR1723600001101234567"],
        ]
    },
)

write(
    "05 Interno/Biljeske-sastanak-2026-01.txt",
    """Sastanak prodaje, 15. sijecnja 2026.

- Cjenik 2026 objavljen, stari cjenik povucen iz upotrebe
- Popust ostaje 5%, prag prometa podignut na 20.000 EUR
- Avans 40% za radove iznad 5.000 EUR - ostaje
- Broj krugova izmjena ponude: dva, treci se naplacuje
- Hotel Adriatic: ponuda poslana, ceka se odgovor
- Pekara Klas: bez popusta, ne ispunjava prag

Sljedeci sastanak: 12. veljace.
""",
)

# -- noise at the root ------------------------------------------------------

pdf_without_text("Skenirani-ugovor-Hotel-Adriatic.pdf")

# A byte-for-byte copy under another name, the way a shared drive collects them.
shutil.copyfile(ROOT / "02 Cjenici/Cjenik-2026.xlsx", ROOT / "02 Cjenici/Cjenik-2026 kopija.xlsx")

# Word's lock file, left behind by a crash.
(ROOT / "03 Ponude/~$Ponuda-2026-0142-Hotel-Adriatic.docx").write_bytes(b"\x00" * 162)

(ROOT / ".DS_Store").write_bytes(b"\x00\x00\x00\x01Bud1" + b"\x00" * 200)

# A logo and a CAD drawing: real files, not ones Knowlith reads.
(ROOT / "Logo-Termoval.png").write_bytes(
    bytes.fromhex(
        "89504e470d0a1a0a0000000d49484452000000100000001008060000001ff3ff"
        "610000001849444154789c63fcffff3f0309a1608410170000d9e10ff3a4f4a4"
        "5e0000000049454e44ae426082"
    )
)
(ROOT / "Nacrt-instalacije-2-kat.dwg").write_bytes(b"AC1032" + b"\x00" * 4096)

# An empty file somebody made and never filled in.
(ROOT / "05 Interno/Plan-2027.docx").write_bytes(b"")

# ---------------------------------------------------------------------------

# Modification times carry meaning: stage 3 decides which of two
# contradicting documents is current by asking the filesystem which one is
# newer. Files written in the same second by this script would make that
# decision a coin flip, and the 2025 terms could "win" over the 2026 ones.
MODIFIED = {
    "01 Uvjeti i pravila/Opci-uvjeti-poslovanja-2025-STARO.docx": "2024-12-18",
    "01 Uvjeti i pravila/Opci-uvjeti-poslovanja-2026.docx": "2025-12-20",
    "01 Uvjeti i pravila/Pravilnik-o-popustima.md": "2025-12-28",
    "01 Uvjeti i pravila/Jamstveni-uvjeti.pdf": "2025-12-20",
    "02 Cjenici/Cjenik-2025-v2.xlsx": "2025-03-11",
    "02 Cjenici/Cjenik-2026.xlsx": "2026-01-08",
    "02 Cjenici/Cjenik-2026 kopija.xlsx": "2026-01-09",
    "02 Cjenici/Cjenik-dobavljaci.csv": "2026-01-10",
    "03 Ponude/Ponuda-2026-0142-Hotel-Adriatic.docx": "2026-01-12",
    "03 Ponude/Ponuda-2026-0143-Pekara-Klas.docx": "2026-01-19",
    "03 Ponude/Predlozak-ponude.docx": "2025-11-04",
    "04 Servis/Servisni-postupak.md": "2025-12-22",
    "04 Servis/Vrijeme-izlaska-na-teren.txt": "2026-01-08",
    "04 Servis/Primopredajni-zapisnik.docx": "2025-09-30",
    "05 Interno/Biljeske-sastanak-2026-01.txt": "2026-01-15",
    "05 Interno/Zaposlenici-place-2026.xlsx": "2026-01-05",
    "Skenirani-ugovor-Hotel-Adriatic.pdf": "2026-01-14",
}

for relative, day in MODIFIED.items():
    target = ROOT / relative
    if not target.exists():
        continue
    when = time.mktime(datetime.fromisoformat(f"{day}T09:30:00").timetuple())
    os.utime(target, (when, when))

files = sorted(p for p in ROOT.rglob("*") if p.is_file())
total = sum(p.stat().st_size for p in files)
print(f"{ROOT}")
print(f"  {len(files)} files, {total / 1024:.0f} KB")
for p in files:
    print(f"    {p.relative_to(ROOT)}")
