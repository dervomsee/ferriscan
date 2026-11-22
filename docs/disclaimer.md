# Disclaimer: reverse engineering and trademarks

> **Summary.** Ferriscan is an independent, unofficial project. It aims to let owners use lawfully
> acquired ScanSnap scanners with other software over Wi-Fi. It is not affiliated with, endorsed by, or
> supported by PFU, Ricoh, or Fujitsu. Use it at your own risk.

## Contents

- [No affiliation](#no-affiliation)
- [Trademarks](#trademarks)
- [How the information was obtained](#how-the-information-was-obtained)
- [Purpose: interoperability](#purpose-interoperability)
- [Captures and personal data](#captures-and-personal-data)
- [No warranty](#no-warranty)

## No affiliation

This project is not affiliated with, authorised, sponsored, or endorsed by PFU Limited, Ricoh, Fujitsu
Limited, or any of their subsidiaries. It is not an official ScanSnap product and receives no support
from the manufacturer. Please do not contact the manufacturer's support about problems with this
software.

## Trademarks

ScanSnap and ScanSnap Home are trademarks or registered trademarks of PFU Limited. Fujitsu is a
trademark of Fujitsu Limited. All other product names and trademarks are the property of their respective
owners.

These names are used only to describe which devices and software this project concerns (nominative use).
Their use does not imply any relationship with, or endorsement by, the trademark owners. The name
"Ferriscan" is not meant to suggest an official manufacturer product.

## How the information was obtained

Ferriscan's initial protocol documentation is adapted from
[ScanSnack](https://github.com/elte0/scansnack), an independent MIT-licensed project by elte0. See
[NOTICE.md](../NOTICE.md) and [LICENSE](../LICENSE) for attribution and license terms. Unless explicitly
stated otherwise, the observations and live tests in those documents were made by ScanSnack, not by
Ferriscan.

ScanSnack describes its protocol information as obtained by observing network traffic between ScanSnap
Home and scanners owned by the author, and by sending requests to those scanners and observing their
responses. Some error texts were cross-checked against the open-source SANE `fujitsu` backend. For the
iX500, scan parameter blocks, page sizes, and Wi-Fi password encoding were established through static
analysis of ScanSnap Home's iX500 Wi-Fi code solely for interoperability. According to the source
documentation, only facts and constants required to communicate with the scanner were recorded; no
ScanSnap Home code was copied.

The independent [bramheerink/scansnap](https://github.com/bramheerink/scansnap) write-up was used by
ScanSnack for comparison only. No code from that GPL-2.0 project is included here. Model-specific sources
are listed in the [iX1500](protocol-ix1500.md#sources-and-conventions) and
[iX500](protocol-ix500.md#sources) write-ups.

This repository contains no manufacturer code, firmware, or binaries. Statements that have not been
confirmed by observation are marked `UNVERIFIED`; inherited open questions must not be presented as
established facts.

## Purpose: interoperability

The purpose of this project is interoperability: to let owners use their own scanner with other operating
systems and programs without the manufacturer's software. Many jurisdictions explicitly allow analysing
a product for this purpose, for example Article 6 of the EU Software Directive (2009/24/EC). This note is
not legal advice.

## Captures and personal data

Raw network captures and test logs must not be published because they may contain scanned documents and
device or network identifiers. Only anonymised protocol extracts without scanned image data or private
identifiers may be used as public fixtures. The local ScanSnack reference checkout is not part of the
Ferriscan Git repository.

## No warranty

The software and documentation are provided "as is", without warranty of any kind (see
[LICENSE](../LICENSE)). Using an unofficial client may put the scanner into an unexpected state, change
its stored users and profiles, or affect the manufacturer's warranty or support. Use it at your own risk.
