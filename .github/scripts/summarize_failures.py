# Turns the merged JUnit report into a single plain-text summary of failed cases.
# The failure detail lives in the <failure message="..."> attribute as an HTML table
# (role | expected | produced) followed by the publisher/subscriber stdout blocks.

import html
import re
import sys
import xml.etree.ElementTree as ET

CELL = re.compile(r"<th>\s*(.*?)\s*</th>", re.DOTALL)
BLOCK = re.compile(r"<strong>\s*(.*?)\s*</strong>(.*?)(?=<strong>|$)", re.DOTALL)


def code_rows(message):
    # The first <table> holds rows of: role | expected code | produced code.
    table = message.split("</table>")[0]
    rows = []
    for row in re.findall(r"<tr>(.*?)</tr>", table, re.DOTALL):
        cells = [c.strip() for c in CELL.findall(row)]
        if len(cells) == 3 and cells[1] != "Expected Code":
            rows.append(tuple(cells))
    return rows


def output_blocks(message):
    # After the table, each app's stdout is a <strong>title</strong> block with <br> line breaks.
    after_table = message.split("</table>", 1)[-1]
    blocks = []
    for title, body in BLOCK.findall(after_table):
        text = re.sub(r"<br\s*/?>", "\n", body)
        text = html.unescape(re.sub(r"<[^>]+>", "", text)).strip()
        blocks.append((title.strip(), text))
    return blocks


def collect(report_path):
    root = ET.parse(report_path).getroot()
    failures = []
    for suite in root:
        suite_name = suite.get("name") or "?"
        for case in suite:
            failure = case.find("failure")
            if failure is None:
                continue
            failures.append((suite_name, case.get("name") or "?", case, failure))
    return failures


def main(report_path, summary_out):
    failures = collect(report_path)

    with open(summary_out, "w", encoding="utf-8") as f:
        f.write("%d failed cases\n\n" % len(failures))
        for suite, case, case_el, failure in sorted(failures):
            message = failure.get("message") or ""
            f.write("=" * 70 + "\n")
            f.write("%s :: %s\n" % (suite, case))

            for role, expected, produced in code_rows(message):
                mark = "  " if expected == produced else "!!"
                f.write("  %s %-12s expected %-24s produced %s\n"
                        % (mark, role, expected, produced))

            for key, value in case_el.attrib.items():
                if "ublisher" in key or "ubscriber" in key:
                    f.write("     %s = %s\n" % (key, value))

            for title, text in output_blocks(message):
                f.write("\n  --- %s ---\n" % title)
                for line in text.splitlines():
                    f.write("  %s\n" % line)
            f.write("\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
