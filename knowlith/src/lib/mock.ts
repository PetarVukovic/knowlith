import type {
  Company,
  SourceDocument,
  ToolRead,
  CompilerRun,
  ContextObject,
  DiscoverySummary,
  Evidence,
  ReviewItem,
  SkillDoc,
  Source,
} from "./types"

/**
 * Demo data for a Croatian HVAC company. Document names, quotes and canonical
 * text stay in the customer's language — evidence quotes are verbatim by
 * definition and must never be translated. Product chrome is English.
 */

export const company: Company = {
  name: "Termoval d.o.o.",
  initials: "TV",
  employees: "18 people",
  industry: "HVAC installation & service",
}

function ev(
  id: string,
  documentName: string,
  locator: string,
  startByte: number,
  quote: string,
  extra: Partial<Evidence> = {},
): Evidence {
  return {
    id,
    documentId: documentName.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
    documentName,
    locator,
    startByte,
    endByte: startByte + quote.length,
    quote,
    verified: true,
    ...extra,
  }
}

export const evidence = {
  discount5: ev(
    "ev-001",
    "Popusti-2026.docx",
    "paragraph 4",
    4218,
    "Komercijalist samostalno odobrava popust do 5% na cijenu iz važećeg cjenika.",
  ),
  discount10: ev(
    "ev-002",
    "Popusti-2026.docx",
    "paragraph 5",
    4297,
    "Popust od 5% do 10% odobrava voditelj prodaje.",
  ),
  discountAbove: ev(
    "ev-003",
    "Popusti-2026.docx",
    "paragraph 6",
    4345,
    "Popust veći od 10% odobrava isključivo direktor, uz pisani trag u ponudi.",
  ),
  discountOld: ev(
    "ev-004",
    "Pravilnik-prodaja-2023.pdf",
    "page 12",
    18904,
    "Komercijalist samostalno odobrava popust do 8%.",
    { page: 12 },
  ),
  discountEmail: ev(
    "ev-005",
    "Dopis-uprava-2026-03.pdf",
    "page 1",
    612,
    "Od 1. travnja 2026. granica samostalnog odobrenja komercijalista je 5%.",
    { page: 1 },
  ),
  validity: ev(
    "ev-010",
    "Ponuda-template.docx",
    "paragraph 11",
    2044,
    "Ponuda vrijedi 14 dana od datuma izdavanja.",
  ),
  payment: ev(
    "ev-011",
    "Opci-uvjeti-2026.docx",
    "section 4.2",
    9120,
    "Rok plaćanja je 15 dana od datuma izdavanja računa.",
  ),
  advance: ev(
    "ev-012",
    "Opci-uvjeti-2026.docx",
    "section 4.3",
    9262,
    "Za radove iznad 5.000,00 EUR naplaćuje se avans od 40%.",
  ),
  warranty: ev(
    "ev-020",
    "Jamstveni-list-ugradnja.pdf",
    "page 2",
    3301,
    "Jamstveni rok na izvedene radove ugradnje iznosi 24 mjeseca.",
    { page: 2 },
  ),
  warrantyService: ev(
    "ev-021",
    "Servis-procedura.docx",
    "paragraph 9",
    5510,
    "Jamstvo prestaje vrijediti ako redoviti servis nije obavljen unutar 12 mjeseci.",
  ),
  response: ev(
    "ev-022",
    "Ugovor-odrzavanje-Konzum.pdf",
    "page 4",
    11208,
    "Izlazak servisera na teren najkasnije 24 sata od prijave kvara.",
    { page: 4 },
  ),
  responsePriority: ev(
    "ev-023",
    "Ugovor-odrzavanje-Konzum.pdf",
    "page 4",
    11290,
    "Za prioritetne objekte rok izlaska je 4 sata.",
    { page: 4 },
  ),
  priceDaikin: ev(
    "ev-030",
    "Cjenik-2026.xlsx",
    "sheet Klima · row 18",
    0,
    "Daikin FTXM35R | split 3,5 kW | 892,00 EUR",
  ),
  priceMontaza: ev(
    "ev-031",
    "Cjenik-2026.xlsx",
    "sheet Usluge · row 7",
    0,
    "Montaža split sustava do 3,5 kW | 210,00 EUR",
  ),
  vatTerm: ev(
    "ev-040",
    "Opci-uvjeti-2026.docx",
    "section 1.3",
    1140,
    "Sve cijene u ovom dokumentu iskazane su bez PDV-a.",
  ),
  handover: ev(
    "ev-041",
    "Servis-procedura.docx",
    "paragraph 3",
    1902,
    "Primopredajni zapisnik potpisuju serviser i predstavnik naručitelja na licu mjesta.",
  ),
  revision: ev(
    "ev-042",
    "Ponuda-template.docx",
    "paragraph 14",
    2410,
    "U cijenu su uključena dva kruga izmjena projektnog rješenja.",
  ),
}

export const contextObjects: ContextObject[] = [
  {
    id: "rule:sales.discount",
    kind: "rule",
    title: "Odobravanje popusta",
    path: "rules/sales/discount.md",
    body: `# Odobravanje popusta

Komercijalist samostalno odobrava popust **do 5%** na cijenu iz važećeg cjenika.

Popust od **5% do 10%** odobrava voditelj prodaje.

Popust **veći od 10%** odobrava isključivo direktor, uz pisani trag u ponudi.`,
    evidence: [evidence.discount5, evidence.discount10, evidence.discountAbove, evidence.discountEmail],
    relations: [
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
      { type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" },
      { type: "supersedes", targetId: "rule:sales.discount@1", targetTitle: "Odobravanje popusta (v1, 8%)" },
      { type: "depends_on", targetId: "fact:price.list-2026", targetTitle: "Cjenik 2026" },
    ],
    confidence: 0.96,
    status: "approved",
    version: 2,
    validFrom: "2026-04-01",
    validTo: null,
    supersedes: "rule:sales.discount@1",
    updatedAt: "2026-09-13T09:12:00Z",
    decidedBy: "Ana Kovač",
    editedOnApproval: false,
  },
  {
    id: "rule:sales.offer-validity",
    kind: "rule",
    title: "Rok valjanosti ponude",
    path: "rules/sales/offer-validity.md",
    body: `# Rok valjanosti ponude

Ponuda vrijedi **14 dana** od datuma izdavanja. Nakon isteka roka cijene se ponovno provjeravaju u važećem cjeniku prije slanja.`,
    evidence: [evidence.validity],
    relations: [
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
      { type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" },
    ],
    confidence: 0.98,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-12T14:40:00Z",
    decidedBy: "Ana Kovač",
  },
  {
    id: "rule:sales.payment-terms",
    kind: "rule",
    title: "Rok plaćanja i avans",
    path: "rules/sales/payment-terms.md",
    body: `# Rok plaćanja i avans

Rok plaćanja je **15 dana** od datuma izdavanja računa.

Za radove iznad **5.000,00 EUR** naplaćuje se avans od **40%** prije početka radova.`,
    evidence: [evidence.payment, evidence.advance],
    relations: [
      { type: "used_by", targetId: "process:finance.invoice", targetTitle: "Izdavanje računa" },
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
    ],
    confidence: 0.94,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-12T14:44:00Z",
    decidedBy: "Ana Kovač",
  },
  {
    id: "rule:service.warranty",
    kind: "rule",
    title: "Jamstveni rok na ugradnju",
    path: "rules/service/warranty.md",
    body: `# Jamstveni rok na ugradnju

Jamstveni rok na izvedene radove ugradnje iznosi **24 mjeseca**.

Jamstvo prestaje vrijediti ako redoviti servis nije obavljen unutar 12 mjeseci.`,
    evidence: [evidence.warranty, evidence.warrantyService],
    relations: [
      { type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" },
      { type: "used_by", targetId: "skill:service-report", targetTitle: "Sastavi servisni izvještaj" },
    ],
    confidence: 0.91,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-11T08:02:00Z",
    decidedBy: "Marko Babić",
  },
  {
    id: "rule:service.response-time",
    kind: "rule",
    title: "Vrijeme izlaska na teren",
    path: "rules/service/response-time.md",
    body: `# Vrijeme izlaska na teren

Izlazak servisera na teren najkasnije **24 sata** od prijave kvara.

Za prioritetne objekte iz ugovora o održavanju rok izlaska je **4 sata**.`,
    evidence: [evidence.response, evidence.responsePriority],
    relations: [
      { type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" },
    ],
    confidence: 0.88,
    status: "draft",
    version: 1,
    validFrom: "2026-09-15",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-15T07:31:00Z",
  },
  {
    id: "rule:sales.revision-rounds",
    kind: "rule",
    title: "Broj krugova izmjena",
    path: "rules/sales/revision-rounds.md",
    body: `# Broj krugova izmjena

U cijenu su uključena **dva kruga izmjena** projektnog rješenja. Svaki daljnji krug naplaćuje se po satnici projektanta.`,
    evidence: [evidence.revision],
    relations: [{ type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" }],
    confidence: 0.83,
    status: "draft",
    version: 1,
    validFrom: "2026-09-15",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-15T07:31:00Z",
  },
  {
    id: "process:sales.quote",
    kind: "process",
    title: "Izrada ponude",
    path: "processes/sales/quote.md",
    body: `# Izrada ponude

1. Zaprimi upit i zabilježi objekt, kvadraturu i željeni termin.
2. Odaberi opremu iz **Cjenika 2026** (list *Klima*).
3. Dodaj montažu iz lista *Usluge*.
4. Primijeni popust prema pravilu *Odobravanje popusta*.
5. Upiši rok valjanosti **14 dana** i uvjete plaćanja.
6. Pošalji ponudu na odobrenje ako popust prelazi ovlast.`,
    evidence: [evidence.validity, evidence.discount5, evidence.priceDaikin, evidence.priceMontaza],
    relations: [
      { type: "depends_on", targetId: "rule:sales.discount", targetTitle: "Odobravanje popusta" },
      { type: "depends_on", targetId: "rule:sales.offer-validity", targetTitle: "Rok valjanosti ponude" },
      { type: "depends_on", targetId: "rule:sales.payment-terms", targetTitle: "Rok plaćanja i avans" },
      { type: "depends_on", targetId: "fact:price.list-2026", targetTitle: "Cjenik 2026" },
      { type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" },
    ],
    confidence: 0.92,
    status: "approved",
    version: 3,
    validFrom: "2026-04-01",
    validTo: null,
    supersedes: "process:sales.quote@2",
    updatedAt: "2026-09-13T09:20:00Z",
    decidedBy: "Ana Kovač",
    editedOnApproval: true,
  },
  {
    id: "process:service.intervention",
    kind: "process",
    title: "Servisni nalog",
    path: "processes/service/intervention.md",
    body: `# Servisni nalog

1. Zaprimi prijavu kvara i odredi prioritet objekta.
2. Dodijeli servisera prema pravilu *Vrijeme izlaska na teren*.
3. Na terenu evidentiraj utrošene sate i ugrađene dijelove.
4. **Primopredajni zapisnik** potpisuju serviser i predstavnik naručitelja na licu mjesta.
5. Provjeri je li zahvat pokriven jamstvom prije izdavanja računa.`,
    evidence: [evidence.handover, evidence.response, evidence.warrantyService],
    relations: [
      { type: "depends_on", targetId: "rule:service.response-time", targetTitle: "Vrijeme izlaska na teren" },
      { type: "depends_on", targetId: "rule:service.warranty", targetTitle: "Jamstveni rok na ugradnju" },
      { type: "used_by", targetId: "skill:service-report", targetTitle: "Sastavi servisni izvještaj" },
    ],
    confidence: 0.89,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-11T08:10:00Z",
    decidedBy: "Marko Babić",
  },
  {
    id: "process:finance.invoice",
    kind: "process",
    title: "Izdavanje računa",
    path: "processes/finance/invoice.md",
    body: `# Izdavanje računa

1. Provjeri je li radni nalog zatvoren i zapisnik potpisan.
2. Obračunaj avans ako je naplaćen.
3. Upiši rok plaćanja **15 dana**.
4. Cijene se u dokumentima vode **bez PDV-a**; PDV se dodaje na računu.`,
    evidence: [evidence.payment, evidence.advance, evidence.vatTerm],
    relations: [
      { type: "depends_on", targetId: "rule:sales.payment-terms", targetTitle: "Rok plaćanja i avans" },
      { type: "depends_on", targetId: "term:bez-pdv", targetTitle: "bez PDV-a" },
    ],
    confidence: 0.95,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-10T16:25:00Z",
    decidedBy: "Ivana Perić",
  },
  {
    id: "term:bez-pdv",
    kind: "term",
    subtype: "term",
    title: "bez PDV-a",
    path: "terms/bez-pdv.md",
    body: `# bez PDV-a

Oznaka uz cijenu koja znači da iznos **ne uključuje porez na dodanu vrijednost**. Sve cijene u cjeniku i ponudama vode se bez PDV-a; PDV se dodaje tek na računu.`,
    evidence: [evidence.vatTerm],
    relations: [
      { type: "used_by", targetId: "process:finance.invoice", targetTitle: "Izdavanje računa" },
      { type: "used_by", targetId: "fact:price.list-2026", targetTitle: "Cjenik 2026" },
    ],
    confidence: 0.99,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-10T16:20:00Z",
    decidedBy: "Ivana Perić",
  },
  {
    id: "term:primopredajni-zapisnik",
    kind: "term",
    subtype: "template",
    title: "primopredajni zapisnik",
    path: "terms/primopredajni-zapisnik.md",
    body: `# primopredajni zapisnik

Dokument koji potvrđuje da je zahvat izveden i preuzet. Potpisuju ga **serviser i predstavnik naručitelja na licu mjesta**. Bez potpisanog zapisnika račun se ne izdaje.`,
    evidence: [evidence.handover],
    relations: [
      { type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" },
    ],
    confidence: 0.97,
    status: "approved",
    version: 1,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: null,
    updatedAt: "2026-09-11T08:04:00Z",
    decidedBy: "Marko Babić",
  },
  {
    id: "fact:price.list-2026",
    kind: "fact",
    subtype: "reference",
    title: "Cjenik 2026",
    path: "facts/price-list-2026.md",
    body: `# Cjenik 2026

Izvor: **Cjenik-2026.xlsx**, listovi *Klima* (128 stavki) i *Usluge* (34 stavke).

Brojevi se ne prepisuju u tekst — agent ih čita izravno iz tablice preko \`lookup_value\`. Ovdje je zabilježeno samo što cjenik jest i kako se čita.

| Stavka | Jedinica | Cijena |
| --- | --- | --- |
| Daikin FTXM35R, split 3,5 kW | kom | 892,00 EUR |
| Montaža split sustava do 3,5 kW | kom | 210,00 EUR |`,
    evidence: [evidence.priceDaikin, evidence.priceMontaza, evidence.vatTerm],
    relations: [
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
      { type: "used_by", targetId: "rule:sales.discount", targetTitle: "Odobravanje popusta" },
    ],
    confidence: 1,
    status: "approved",
    version: 4,
    validFrom: "2026-01-01",
    validTo: null,
    supersedes: "fact:price.list-2025",
    updatedAt: "2026-09-14T11:00:00Z",
    decidedBy: "Ana Kovač",
  },
]

export const skills: SkillDoc[] = [
  {
    id: "skill:quote-hvac",
    name: "Izradi ponudu za klimatizaciju",
    description:
      "Builds a priced quote for a split or multi-split installation using the current price list and the approved discount rule.",
    markdown: `# Izradi ponudu za klimatizaciju

Koristi se kad klijent traži ponudu za ugradnju klima uređaja.

## Postupak

1. Utvrdi kvadraturu prostora i potreban učin (kW).
2. Dohvati cijenu opreme iz **Cjenika 2026**, list *Klima*.
3. Dohvati cijenu montaže iz lista *Usluge*.
4. Zbroji stavke. Cijene su **bez PDV-a**.
5. Primijeni popust unutar ovlasti korisnika:
   - do 5% — komercijalist
   - 5–10% — voditelj prodaje
   - iznad 10% — direktor
6. Upiši rok valjanosti **14 dana** i rok plaćanja **15 dana**.
7. Ako je iznos veći od 5.000,00 EUR, dodaj avans **40%**.

## Što nikad ne radi

Ne procjenjuje cijenu koje nema u cjeniku i ne interpolira između dvije stavke. Ako stavka ne postoji, vrati što nedostaje.`,
    requires: [
      { type: "depends_on", targetId: "rule:sales.discount", targetTitle: "Odobravanje popusta" },
      { type: "depends_on", targetId: "rule:sales.offer-validity", targetTitle: "Rok valjanosti ponude" },
      { type: "depends_on", targetId: "rule:sales.payment-terms", targetTitle: "Rok plaćanja i avans" },
      { type: "depends_on", targetId: "fact:price.list-2026", targetTitle: "Cjenik 2026" },
      { type: "depends_on", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
    ],
    inputs: [
      { name: "area_m2", type: "number", description: "Površina prostora u m²." },
      { name: "unit_model", type: "string", description: "Model uređaja, ako ga klijent traži." },
      { name: "discount_pct", type: "number", description: "Traženi popust u postotku." },
      { name: "requester_role", type: "enum", description: "komercijalist | voditelj | direktor" },
    ],
    outputs: [
      { name: "line_items", type: "table", description: "Stavke s količinom i cijenom bez PDV-a." },
      { name: "total_eur", type: "number", description: "Ukupno bez PDV-a." },
      { name: "approval_required", type: "boolean", description: "Treba li ponuda odobrenje nadređenog." },
      { name: "valid_until", type: "date", description: "Datum isteka ponude." },
    ],
    affects: [
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
      { type: "used_by", targetId: "process:finance.invoice", targetTitle: "Izdavanje računa" },
    ],
    evidence: [evidence.discount5, evidence.validity, evidence.payment, evidence.priceDaikin],
    status: "approved",
    confidence: 0.91,
    decidedBy: "Ana Kovač",
    version: 2,
    updatedAt: "2026-09-13T10:02:00Z",
  },
  {
    id: "skill:service-report",
    name: "Sastavi servisni izvještaj",
    description:
      "Turns a technician's field notes into a handover record that matches the service process and warranty rule.",
    markdown: `# Sastavi servisni izvještaj

Pretvara bilješke servisera u **primopredajni zapisnik**.

## Postupak

1. Zapiši objekt, datum i ime servisera.
2. Popiši utrošene sate i ugrađene dijelove.
3. Provjeri pokriva li zahvat jamstvo (24 mjeseca, uz redoviti servis unutar 12 mjeseci).
4. Označi je li potreban ponovni izlazak.
5. Ostavi mjesto za potpis servisera i predstavnika naručitelja.`,
    requires: [
      { type: "depends_on", targetId: "process:service.intervention", targetTitle: "Servisni nalog" },
      { type: "depends_on", targetId: "rule:service.warranty", targetTitle: "Jamstveni rok na ugradnju" },
      { type: "depends_on", targetId: "term:primopredajni-zapisnik", targetTitle: "primopredajni zapisnik" },
    ],
    inputs: [
      { name: "site", type: "string", description: "Objekt na kojem je zahvat izveden." },
      { name: "hours", type: "number", description: "Utrošeni sati rada." },
      { name: "parts", type: "list", description: "Ugrađeni dijelovi." },
    ],
    outputs: [
      { name: "report_md", type: "markdown", description: "Zapisnik spreman za potpis." },
      { name: "warranty_covered", type: "boolean", description: "Je li zahvat pokriven jamstvom." },
    ],
    affects: [{ type: "used_by", targetId: "process:finance.invoice", targetTitle: "Izdavanje računa" }],
    evidence: [evidence.handover, evidence.warranty, evidence.warrantyService],
    status: "approved",
    confidence: 0.86,
    decidedBy: "Marko Babić",
    version: 1,
    updatedAt: "2026-09-11T09:15:00Z",
  },
  {
    id: "skill:check-discount",
    name: "Provjeri ovlast za popust",
    description: "Answers whether a given discount can be approved by the person asking, and by whom otherwise.",
    markdown: `# Provjeri ovlast za popust

Ulaz je traženi popust i uloga tražitelja. Izlaz je tko smije odobriti.

| Popust | Odobrava |
| --- | --- |
| do 5% | komercijalist |
| 5–10% | voditelj prodaje |
| iznad 10% | direktor, uz pisani trag |`,
    requires: [{ type: "depends_on", targetId: "rule:sales.discount", targetTitle: "Odobravanje popusta" }],
    inputs: [
      { name: "discount_pct", type: "number", description: "Traženi popust." },
      { name: "requester_role", type: "enum", description: "Uloga osobe koja traži." },
    ],
    outputs: [
      { name: "allowed", type: "boolean", description: "Smije li tražitelj sam odobriti." },
      { name: "approver", type: "string", description: "Tko mora odobriti ako ne smije." },
    ],
    affects: [{ type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" }],
    evidence: [evidence.discount5, evidence.discount10, evidence.discountAbove],
    status: "draft",
    confidence: 0.62,
    decidedBy: null,
    version: 1,
    updatedAt: "2026-09-15T07:33:00Z",
  },
]

export const reviewQueue: ReviewItem[] = [
  {
    id: "rev-001",
    objectId: "rule:sales.discount",
    kind: "rule",
    title: "Odobravanje popusta",
    before: `# Odobravanje popusta

Komercijalist samostalno odobrava popust do 8% na cijenu iz važećeg cjenika.

Popust veći od 8% odobrava voditelj prodaje.`,
    after: `# Odobravanje popusta

Komercijalist samostalno odobrava popust do 5% na cijenu iz važećeg cjenika.

Popust od 5% do 10% odobrava voditelj prodaje.

Popust veći od 10% odobrava isključivo direktor, uz pisani trag u ponudi.`,
    evidence: [evidence.discount5, evidence.discount10, evidence.discountAbove, evidence.discountEmail],
    confidence: 0.96,
    affects: [
      { type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" },
      { type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" },
      { type: "used_by", targetId: "skill:check-discount", targetTitle: "Provjeri ovlast za popust" },
    ],
    conflict: {
      summary:
        "Two documents state a different self-approval limit. The older one is still circulating in the Prodaja folder.",
      sides: [
        { label: "Pravilnik-prodaja-2023.pdf", value: "8%", evidence: evidence.discountOld },
        { label: "Popusti-2026.docx", value: "5%", evidence: evidence.discount5 },
      ],
    },
    compiledAt: "2026-09-15T07:28:00Z",
  },
  {
    id: "rev-002",
    objectId: "rule:service.response-time",
    kind: "rule",
    title: "Vrijeme izlaska na teren",
    before: null,
    after: `# Vrijeme izlaska na teren

Izlazak servisera na teren najkasnije 24 sata od prijave kvara.

Za prioritetne objekte iz ugovora o održavanju rok izlaska je 4 sata.`,
    evidence: [evidence.response, evidence.responsePriority],
    confidence: 0.88,
    affects: [{ type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" }],
    compiledAt: "2026-09-15T07:29:00Z",
  },
  {
    id: "rev-003",
    objectId: "rule:sales.revision-rounds",
    kind: "rule",
    title: "Broj krugova izmjena",
    before: null,
    after: `# Broj krugova izmjena

U cijenu su uključena dva kruga izmjena projektnog rješenja. Svaki daljnji krug naplaćuje se po satnici projektanta.`,
    evidence: [evidence.revision],
    confidence: 0.83,
    affects: [{ type: "used_by", targetId: "process:sales.quote", targetTitle: "Izrada ponude" }],
    compiledAt: "2026-09-15T07:29:00Z",
  },
  {
    id: "rev-004",
    objectId: "rule:service.warranty",
    kind: "rule",
    title: "Jamstveni rok na ugradnju",
    before: `# Jamstveni rok na ugradnju

Jamstveni rok na izvedene radove ugradnje iznosi 24 mjeseca.`,
    after: `# Jamstveni rok na ugradnju

Jamstveni rok na izvedene radove ugradnje iznosi 24 mjeseca.

Jamstvo prestaje vrijediti ako redoviti servis nije obavljen unutar 12 mjeseci.`,
    evidence: [evidence.warranty, evidence.warrantyService],
    confidence: 0.91,
    affects: [
      { type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" },
      { type: "used_by", targetId: "skill:service-report", targetTitle: "Sastavi servisni izvještaj" },
    ],
    compiledAt: "2026-09-15T07:30:00Z",
  },
  {
    id: "rev-005",
    objectId: "term:primopredajni-zapisnik",
    kind: "term",
    title: "primopredajni zapisnik",
    before: null,
    after: `# primopredajni zapisnik

Dokument koji potvrđuje da je zahvat izveden i preuzet. Potpisuju ga serviser i predstavnik naručitelja na licu mjesta. Bez potpisanog zapisnika račun se ne izdaje.`,
    evidence: [evidence.handover],
    confidence: 0.97,
    affects: [{ type: "used_by", targetId: "process:service.intervention", targetTitle: "Servisni nalog" }],
    compiledAt: "2026-09-15T07:30:00Z",
  },
  {
    id: "rev-006",
    objectId: "process:sales.quote",
    kind: "process",
    title: "Izrada ponude",
    before: `# Izrada ponude

1. Zaprimi upit.
2. Odaberi opremu iz cjenika.
3. Pošalji ponudu.`,
    after: `# Izrada ponude

1. Zaprimi upit i zabilježi objekt, kvadraturu i željeni termin.
2. Odaberi opremu iz Cjenika 2026 (list Klima).
3. Dodaj montažu iz lista Usluge.
4. Primijeni popust prema pravilu Odobravanje popusta.
5. Upiši rok valjanosti 14 dana i uvjete plaćanja.
6. Pošalji ponudu na odobrenje ako popust prelazi ovlast.`,
    evidence: [evidence.validity, evidence.discount5, evidence.priceDaikin],
    confidence: 0.92,
    affects: [{ type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" }],
    compiledAt: "2026-09-15T07:31:00Z",
  },
  {
    id: "rev-007",
    objectId: "skill:check-discount",
    kind: "skill",
    title: "Provjeri ovlast za popust",
    before: null,
    after: `# Provjeri ovlast za popust

Ulaz je traženi popust i uloga tražitelja. Izlaz je tko smije odobriti.

| Popust | Odobrava |
| --- | --- |
| do 5% | komercijalist |
| 5–10% | voditelj prodaje |
| iznad 10% | direktor, uz pisani trag |`,
    evidence: [evidence.discount5, evidence.discount10, evidence.discountAbove],
    confidence: 0.9,
    affects: [{ type: "used_by", targetId: "skill:quote-hvac", targetTitle: "Izradi ponudu za klimatizaciju" }],
    compiledAt: "2026-09-15T07:32:00Z",
  },
]

export const sources: Source[] = [
  {
    id: "src-prodaja",
    name: "Prodaja",
    path: "/Users/ana/Termoval/Prodaja",
    kind: "folder",
    access: "read-only",
    fileCount: 1284,
    bytes: 4_512_000_000,
    lastSync: "2026-09-15T07:33:00Z",
    lastAnalyzed: "2026-09-15T07:33:00Z",
    changesFound: 5,
    conflictsFound: 1,
    processor: "codex",
    status: "active",
    fileTypes: [
      { ext: "PDF", count: 612 },
      { ext: "DOCX", count: 428 },
      { ext: "XLSX", count: 196 },
      { ext: "CSV", count: 48 },
    ],
  },
  {
    id: "src-nas",
    name: "Zajedničko (NAS)",
    path: "\\\\termoval-nas\\Zajednicko",
    kind: "nas",
    access: "read-only",
    fileCount: 986,
    bytes: 3_180_000_000,
    lastSync: "2026-09-15T05:10:00Z",
    lastAnalyzed: "2026-09-15T05:10:00Z",
    changesFound: 2,
    conflictsFound: 0,
    processor: "codex",
    status: "active",
    fileTypes: [
      { ext: "PDF", count: 501 },
      { ext: "DOCX", count: 302 },
      { ext: "XLSX", count: 141 },
      { ext: "CSV", count: 42 },
    ],
  },
  {
    id: "src-servis",
    name: "Servis",
    path: "/Users/ana/Termoval/Servis",
    kind: "folder",
    access: "read-write",
    fileCount: 168,
    bytes: 712_000_000,
    lastSync: "2026-09-14T18:02:00Z",
    lastAnalyzed: "2026-09-14T18:02:00Z",
    changesFound: 0,
    conflictsFound: 0,
    processor: "managed",
    status: "paused",
    fileTypes: [
      { ext: "DOCX", count: 96 },
      { ext: "PDF", count: 58 },
      { ext: "XLSX", count: 14 },
    ],
  },
]

export const discovery: DiscoverySummary = {
  rules: 12,
  processes: 4,
  terms: 37,
  skills: 3,
  conflicts: 7,
  filesRead: 2438,
  spansExtracted: 18_204,
  durationSeconds: 1_147,
}

export const compilerRuns: CompilerRun[] = [
  {
    id: "run-2026-09-15-07",
    startedAt: "2026-09-15T07:14:00Z",
    durationSeconds: 1147,
    processor: "codex",
    filesProcessed: 2438,
    candidates: 286,
    accepted: 56,
    rejectedUnsupported: 41,
    rejectedDuplicate: 189,
    stage: "done",
  },
  {
    id: "run-2026-09-14-11",
    startedAt: "2026-09-14T11:00:00Z",
    durationSeconds: 96,
    processor: "codex",
    filesProcessed: 12,
    candidates: 18,
    accepted: 4,
    rejectedUnsupported: 2,
    rejectedDuplicate: 12,
    stage: "done",
  },
  {
    id: "run-2026-09-13-09",
    startedAt: "2026-09-13T09:02:00Z",
    durationSeconds: 208,
    processor: "managed",
    filesProcessed: 31,
    candidates: 44,
    accepted: 9,
    rejectedUnsupported: 6,
    rejectedDuplicate: 29,
    stage: "done",
  },
]

/**
 * Excerpts of the original files, as the daemon extracted them. Clicking an
 * evidence quote opens the document here with that block highlighted, so a
 * person can check the claim against the paragraph it came from.
 */
export const sourceDocuments: SourceDocument[] = [
  {
    name: "Popusti-2026.docx",
    kind: "docx",
    path: "/Users/ana/Termoval/Prodaja/Pravilnici/Popusti-2026.docx",
    modified: "2026-03-28T10:12:00Z",
    blocks: [
      { locator: "paragraph 1", heading: true, text: "Pravilnik o odobravanju popusta" },
      {
        locator: "paragraph 2",
        text: "Ovaj pravilnik primjenjuje se na sve ponude izdane od 1. travnja 2026. i zamjenjuje odredbe o popustima iz Pravilnika o prodaji iz 2023.",
      },
      { locator: "paragraph 3", heading: true, text: "1. Ovlasti za odobravanje" },
      { locator: "paragraph 4", text: "Komercijalist samostalno odobrava popust do 5% na cijenu iz važećeg cjenika." },
      { locator: "paragraph 5", text: "Popust od 5% do 10% odobrava voditelj prodaje." },
      { locator: "paragraph 6", text: "Popust veći od 10% odobrava isključivo direktor, uz pisani trag u ponudi." },
      {
        locator: "paragraph 7",
        text: "Odobrenje se bilježi u napomeni ponude s imenom osobe koja je popust odobrila i datumom.",
      },
    ],
  },
  {
    name: "Pravilnik-prodaja-2023.pdf",
    kind: "pdf",
    path: "/Users/ana/Termoval/Prodaja/Arhiva/Pravilnik-prodaja-2023.pdf",
    modified: "2023-01-09T08:00:00Z",
    blocks: [
      { locator: "page 12", heading: true, text: "Poglavlje 4 — Cijene i popusti" },
      { locator: "page 12 · paragraph 2", text: "Komercijalist samostalno odobrava popust do 8%." },
      { locator: "page 12 · paragraph 3", text: "Popust veći od 8% odobrava voditelj prodaje." },
    ],
  },
  {
    name: "Dopis-uprava-2026-03.pdf",
    kind: "pdf",
    path: "/Users/ana/Termoval/Prodaja/Dopisi/Dopis-uprava-2026-03.pdf",
    modified: "2026-03-20T14:30:00Z",
    blocks: [
      { locator: "page 1", heading: true, text: "Dopis uprave — ožujak 2026." },
      { locator: "page 1 · paragraph 2", text: "Od 1. travnja 2026. granica samostalnog odobrenja komercijalista je 5%." },
      { locator: "page 1 · paragraph 3", text: "Molimo voditelje da o promjeni obavijeste svoje timove." },
    ],
  },
  {
    name: "Ponuda-template.docx",
    kind: "docx",
    path: "/Users/ana/Termoval/Prodaja/Predlosci/Ponuda-template.docx",
    modified: "2026-01-15T09:40:00Z",
    blocks: [
      { locator: "paragraph 10", heading: true, text: "Uvjeti ponude" },
      { locator: "paragraph 11", text: "Ponuda vrijedi 14 dana od datuma izdavanja." },
      { locator: "paragraph 12", text: "Cijene su iskazane bez PDV-a." },
      { locator: "paragraph 14", text: "U cijenu su uključena dva kruga izmjena projektnog rješenja." },
    ],
  },
  {
    name: "Opci-uvjeti-2026.docx",
    kind: "docx",
    path: "\\\\termoval-nas\\Zajednicko\\Pravni\\Opci-uvjeti-2026.docx",
    modified: "2026-01-02T11:05:00Z",
    blocks: [
      { locator: "section 1.3", text: "Sve cijene u ovom dokumentu iskazane su bez PDV-a." },
      { locator: "section 4.1", heading: true, text: "4. Plaćanje" },
      { locator: "section 4.2", text: "Rok plaćanja je 15 dana od datuma izdavanja računa." },
      { locator: "section 4.3", text: "Za radove iznad 5.000,00 EUR naplaćuje se avans od 40%." },
    ],
  },
  {
    name: "Jamstveni-list-ugradnja.pdf",
    kind: "pdf",
    path: "\\\\termoval-nas\\Zajednicko\\Servis\\Jamstveni-list-ugradnja.pdf",
    modified: "2026-02-11T07:20:00Z",
    blocks: [
      { locator: "page 2", text: "Jamstveni rok na izvedene radove ugradnje iznosi 24 mjeseca." },
      { locator: "page 2 · paragraph 4", text: "Jamstvo se odnosi na rad i materijal koji je ugradio izvođač." },
    ],
  },
  {
    name: "Servis-procedura.docx",
    kind: "docx",
    path: "/Users/ana/Termoval/Servis/Servis-procedura.docx",
    modified: "2026-02-28T15:55:00Z",
    blocks: [
      { locator: "paragraph 3", text: "Primopredajni zapisnik potpisuju serviser i predstavnik naručitelja na licu mjesta." },
      { locator: "paragraph 8", heading: true, text: "Jamstvo" },
      { locator: "paragraph 9", text: "Jamstvo prestaje vrijediti ako redoviti servis nije obavljen unutar 12 mjeseci." },
    ],
  },
  {
    name: "Ugovor-odrzavanje-Konzum.pdf",
    kind: "pdf",
    path: "\\\\termoval-nas\\Zajednicko\\Ugovori\\Ugovor-odrzavanje-Konzum.pdf",
    modified: "2026-01-30T13:10:00Z",
    blocks: [
      { locator: "page 4", heading: true, text: "Članak 6 — Odzivno vrijeme" },
      { locator: "page 4 · rok", text: "Izlazak servisera na teren najkasnije 24 sata od prijave kvara." },
      { locator: "page 4 · prioritet", text: "Za prioritetne objekte rok izlaska je 4 sata." },
    ],
  },
  {
    name: "Cjenik-2026.xlsx",
    kind: "xlsx",
    path: "/Users/ana/Termoval/Prodaja/Cjenik-2026.xlsx",
    modified: "2026-09-14T10:55:00Z",
    columns: ["Šifra", "Naziv", "Jedinica", "Cijena (EUR)"],
    blocks: [
      { locator: "sheet Klima · row 16", cells: ["KL-031", "Daikin FTXM25R, split 2,5 kW", "kom", "784,00"] },
      { locator: "sheet Klima · row 17", cells: ["KL-032", "Daikin FTXM30R, split 3,0 kW", "kom", "836,00"] },
      { locator: "sheet Klima · row 18", cells: ["KL-033", "Daikin FTXM35R, split 3,5 kW", "kom", "892,00"] },
      { locator: "sheet Klima · row 19", cells: ["KL-034", "Daikin FTXM42R, split 4,2 kW", "kom", "1.048,00"] },
      { locator: "sheet Usluge · row 7", cells: ["US-007", "Montaža split sustava do 3,5 kW", "kom", "210,00"] },
    ],
  },
]

/**
 * Reads recorded by the MCP gateway. These are events that happened, not
 * subscriptions — the UI says "read by", never "used by", for this data.
 */
export const toolReads: Record<string, ToolRead[]> = {
  "rule:sales.discount": [
    { tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" },
    { tool: "Codex CLI", lastReadAt: "2026-09-15T12:05:00Z" },
    { tool: "Cursor", lastReadAt: "2026-09-14T09:18:00Z" },
  ],
  "rule:sales.offer-validity": [
    { tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" },
    { tool: "Codex CLI", lastReadAt: "2026-09-13T16:30:00Z" },
  ],
  "rule:sales.payment-terms": [{ tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" }],
  "term:bez-pdv": [
    { tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" },
    { tool: "Codex CLI", lastReadAt: "2026-09-15T12:05:00Z" },
  ],
  "fact:price.list-2026": [
    { tool: "Claude Desktop", lastReadAt: "2026-09-15T17:43:00Z" },
    { tool: "Codex CLI", lastReadAt: "2026-09-15T12:06:00Z" },
    { tool: "Cursor", lastReadAt: "2026-09-15T08:55:00Z" },
  ],
  "process:sales.quote": [{ tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" }],
  "skill:quote-hvac": [
    { tool: "Claude Desktop", lastReadAt: "2026-09-15T17:42:00Z" },
    { tool: "Cursor", lastReadAt: "2026-09-12T11:20:00Z" },
  ],
  "rule:service.warranty": [{ tool: "Codex CLI", lastReadAt: "2026-09-11T14:02:00Z" }],
}

/** Source-level activity for the home screen. */
export const recentActivity = [
  {
    id: "act-1",
    title: "Pricing rules updated",
    detail: "Popusti-2026.docx replaced the 2023 limit of 8% with 5%.",
    source: "Prodaja",
    at: "2026-09-15T07:28:00Z",
    tone: "conflict" as const,
  },
  {
    id: "act-2",
    title: "New service process discovered",
    detail: "Servisni nalog, found in Servis-procedura.docx.",
    source: "Servis",
    at: "2026-09-15T07:30:00Z",
    tone: "pending" as const,
  },
  {
    id: "act-3",
    title: "Cjenik 2026 re-read",
    detail: "128 items on sheet Klima, 34 on Usluge. No rule changed.",
    source: "Prodaja",
    at: "2026-09-14T11:00:00Z",
    tone: "neutral" as const,
  },
]
