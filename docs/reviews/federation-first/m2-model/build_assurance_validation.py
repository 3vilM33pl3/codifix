#!/usr/bin/env python3
"""Independent arithmetic fixtures for the counsel and assurance-lead revision.

These bounded cases use explicit employee-month and monthly-expense arithmetic.
They are not a second forecast engine. All thirty prior M2 fixtures are retained
with their original expected values; only the newly introduced employee and
counsel are disabled in those regression cases. Existing fictional overrides,
including accounting/payroll amounts, remain unchanged.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path

from build_ai_validation import build_cases as build_ai_cases, checks, lean_profile

ROOT = Path(__file__).resolve().parent
STAFF_ROWS = (*range(5, 23), 46)


def no_staff():
    return {f"Team!AQ{row}": 0 for row in STAFF_ROWS}


def assurance_employee(hire=3):
    return {**no_staff(), "Team!AQ46": 1, "Team!AM46": hire,
            "Team!AN46": 0, "Team!B46": 120000}


def build_cases():
    prior_ai, prior_v1 = build_ai_cases()
    regressions = copy.deepcopy(prior_v1 + prior_ai)
    for case in regressions:
        case["inputs"].update({"Team!AQ46": 0, "Assumptions!B126": 0,
                               "Assumptions!B127": 1, "Assumptions!B128": 0})
        case["notes"] += (" Counsel/assurance revision regression: the new role and "
                          "counsel are excluded; previous expected arithmetic and "
                          "fictional accounting/payroll overrides are preserved.")

    base = copy.deepcopy(prior_ai[0]["inputs"])
    base.update({**no_staff(), "Team!AM46": 3, "Team!AN46": 0,
                 "Assumptions!B126": 0, "Assumptions!B127": 1,
                 "Assumptions!B128": 0})
    result = []

    def add(id_, label, overrides, expected, note):
        result.append(dict(id="assurance_" + id_, label=label,
                           inputs={**base, **overrides}, checks=checks(expected),
                           notes=note))

    add("nine_staff_ai_funding", "Nine-role hiring profile: 168 and 276 employee-months", {
        **lean_profile(), "Team!AQ46": 1,
    }, [
        ("Team!C27", 2, "Two employees in month 1"),
        ("Team!D27", 3, "Three employees in month 2"),
        ("Team!E27", 4, "Assurance lead starts in month 3"),
        ("Summary!C18", 8, "Eight employees at first year end"),
        ("Summary!D18", 9, "Nine employees at second year end"),
        ("Summary!E18", 9, "Nine employees at third year end"),
        ("Summary!B72", 840000, "168 employee-months times 5000 allowance"),
        ("Summary!B73", 420000, "168 employee-months times 2500 expected spend"),
        ("Summary!B74", 420000, "Unused first 24-month allowance remains cash"),
        ("Summary!B75", 1380000, "276 employee-months times 5000 allowance"),
        ("Summary!B76", 690000, "276 employee-months times 2500 spending"),
        ("Summary!B77", 690000, "Full-horizon unused allowance is disclosed separately"),
        ("Summary!B59", 55000, "22 assurance-lead months times 2500 G&A AI"),
        ("Summary!B42", 420000, "Cash uses include expected AI spending only"),
        ("Summary!B25", 67500, "Following quarter: nine staff times three months times 2500"),
        ("Summary!B22", 907500, "420000 spending plus 420000 headroom plus 67500 buffer"),
        ("Summary!B27", 487500, "Month 24 cash retains headroom and operating buffer"),
        ("Summary!B78", 67500, "Cash outside the AI earmark equals the operating buffer"),
        ("Summary!B84", 487500, "Required cash includes the AI earmark once"),
    ], "AI-only fictional company isolates headcount arithmetic. The prior eight-role "
       "profile contributes 146/242 employee-months; the month-3 lead adds 22/34. "
       "Salary multiplier and unrelated costs are zero solely for this fixture.")

    paid_employee = {
        **assurance_employee(), "Assumptions!B62": 1,
        "Assumptions!B68": 1, "Assumptions!B70": 1,
        "Assumptions!B72": 0.1, "Assumptions!B73": 1,
        "Assumptions!B74": 100, "Assumptions!B75": 200,
    }
    add("single_ga_employee", "One month-3 G&A employee incurs costs and AI once", paid_employee, [
        ("Team!D27", 0, "No employee before the hire month"),
        ("Team!E27", 1, "One included employee starts in month 3"),
        ("Team!E46", 10000, "120000 salary divided by twelve months"),
        ("Team!E24", 10000, "Total salary includes the new role once"),
        ("Team!E25", 0, "Assurance role has no reserved delivery salary"),
        ("Team!E26", 0, "Assurance role supplies no assumed delivery capacity"),
        ("Team!E28", 10000, "All assurance salary is operating payroll"),
        ("Team!E29", 0, "Assurance salary is not classified as Product"),
        ("Team!E30", 0, "Assurance salary is not classified as GTM"),
        ("Team!E33", 10000, "Assurance salary is G&A payroll"),
        ("Team!E31", 12000, "Recruitment charged once at hire"),
        ("Team!F31", 0, "Recruitment does not repeat"),
        ("'Employee AI'!D17", 0, "No employee AI expense before hire"),
        ("'Employee AI'!E14", 5000, "One employee monthly allowance"),
        ("'Employee AI'!E15", 250, "Subscription is included within expected spend"),
        ("'Employee AI'!E16", 2250, "Metered usage excludes subscription spending"),
        ("'Employee AI'!E17", 2500, "Monthly expected AI expense"),
        ("'Employee AI'!E18", 2500, "Unused monthly allowance remains cash headroom"),
        ("'Employee AI'!E20", 0, "No delivery AI allocation"),
        ("'Employee AI'!E21", 0, "No Product AI allocation"),
        ("'Employee AI'!E22", 0, "No GTM AI allocation"),
        ("'Employee AI'!E29", 2500, "Full assurance AI expense belongs to G&A"),
        ("'Employee AI'!E23", 2500, "All functional AI allocations reconcile"),
        ("'Employee AI'!E24", 0, "No unallocated or duplicated AI spending"),
        ("Costs!E34", 0, "G&A role does not create cost of sales"),
        ("'P&L & Cash'!E12", -100, "Ordinary software tools start at hire"),
        ("'P&L & Cash'!E13", -200, "Workspace starts at hire"),
        ("'P&L & Cash'!E38", -2500, "G&A AI enters operating expense once"),
        ("'P&L & Cash'!E25", 24800, "Salary 10000, AI 2500, overhead 300 and recruitment 12000"),
        ("'P&L & Cash'!F25", 12800, "Only recurring employee costs repeat"),
        ("Summary!B48", 220000, "22 months of 10000 G&A payroll"),
        ("Summary!B59", 55000, "22 months of 2500 G&A AI expense"),
        ("Summary!B73", 55000, "First 24-month expected AI spending"),
        ("Summary!B74", 55000, "First 24-month unused AI allowance"),
        ("Summary!B42", 293600, "22 times 12800 plus 12000 recruitment"),
        ("Summary!B25", 38400, "Three following months of 12800 operating costs"),
        ("Summary!B22", 387000, "Cash expense plus operating buffer plus AI headroom"),
        ("Summary!B27", 93400, "Closing cash retains operating buffer and AI headroom"),
    ], "The assurance employee is a distinct G&A role, not a relabelled engineering "
       "position. All original staff retain positive salaries but are excluded.")

    add("lead_hiring_delay", "Employee hiring delay postpones the new role and every linked cost", {
        **paid_employee, "Assumptions!B69": 2,
    }, [
        ("Team!F27", 0, "Month-3 hire delayed beyond month 4"),
        ("Team!G27", 1, "Employee begins in month 5"),
        ("Team!F46", 0, "No salary before delayed start"),
        ("Team!G33", 10000, "G&A salary begins in month 5"),
        ("Team!F31", 0, "Recruitment is not charged at original hire timing"),
        ("Team!G31", 12000, "Recruitment is charged at delayed hire"),
        ("'Employee AI'!F29", 0, "No AI expense before delayed start"),
        ("'Employee AI'!G29", 2500, "G&A AI starts with employment"),
        ("'P&L & Cash'!F12", 0, "No tools before employment"),
        ("'P&L & Cash'!G12", -100, "Tools start with delayed employment"),
        ("Summary!B73", 50000, "Twenty employee-months expected AI expense"),
        ("Summary!B74", 50000, "Twenty employee-months unused AI allowance"),
        ("Summary!B42", 268000, "Twenty times 12800 plus 12000 recruitment"),
        ("Summary!B22", 356400, "268000 expense plus 38400 buffer plus 50000 headroom"),
        ("Summary!B27", 88400, "Month 24 cash preserves buffer and headroom"),
    ], "The common two-month employee hiring delay applies exactly once. This is a "
       "timing test; it does not establish certification-readiness timing.")

    add("lead_excluded", "Excluded assurance lead generates no employee-linked spending", {
        **paid_employee, "Team!AQ46": 0, "Team!AN46": 0.5,
    }, [
        ("Summary!C18", 0, "No year-one headcount"),
        ("Summary!E18", 0, "No year-three headcount"),
        ("Team!E46", 0, "Positive salary does not bypass inclusion control"),
        ("Team!E33", 0, "No G&A payroll"),
        ("Team!E26", 0, "Positive delivery share does not create excluded capacity"),
        ("Team!E31", 0, "No recruitment fee for an excluded role"),
        ("'P&L & Cash'!E12", 0, "No software cost"),
        ("'P&L & Cash'!E13", 0, "No workspace cost"),
        ("'Employee AI'!E29", 0, "No G&A AI expense"),
        ("Summary!B72", 0, "No employee AI allowance"),
        ("Summary!B74", 0, "No headroom for excluded roles"),
        ("Summary!B42", 0, "No employee cash uses"),
        ("Summary!B22", 0, "No funding needed for the isolated zero-cost company"),
    ], "All salary, allocation, recruitment, tools, workspace and AI inputs are "
       "positive. The role inclusion control must suppress every linked amount.")

    add("historical_profile_excludes_lead", "Historical staffing excludes the newly introduced role", {
        "Assumptions!B118": 1, "Assumptions!B68": 1,
        "Team!AQ46": 1, "Team!AM46": 1, "Team!B46": 1200000,
    }, [
        ("Team!C46", 0, "Historical profile ignores active-profile lead inclusion"),
        ("Team!AL46", 0, "New lead remains absent throughout historical horizon"),
        ("Team!AL33", 0, "Historical original staff contain no new G&A lead payroll"),
        ("'Employee AI'!C29", 0, "No G&A AI allowance for the historically absent lead"),
        ("Summary!C18", 10, "Original first-year headcount retained"),
        ("Summary!D18", 16, "Original second-year headcount retained"),
        ("Summary!E18", 18, "Original third-year headcount retained"),
        ("Summary!B72", 1190000, "238 historical employee-months of allowance"),
        ("Summary!B73", 595000, "238 historical employee-months of expected spending"),
        ("Summary!B75", 2250000, "450 historical employee-months of allowance"),
        ("Summary!B79", 0, "Historical profile has no payroll saving against itself"),
    ], "The new lead's active-profile salary and inclusion are deliberately large "
       "and positive. Immutable historical inclusion zero must prevail.")

    legal = {"Assumptions!B126": 5000, "Assumptions!B127": 1,
             "Assumptions!B128": 1}
    add("fractional_counsel_only", "Fractional counsel is an external expense with no employee AI", {
        **legal, "Assumptions!B68": 1, "Assumptions!B72": 0.3,
        "Assumptions!B74": 123, "Assumptions!B75": 234,
    }, [
        ("'P&L & Cash'!C39", -5000, "Monthly counsel expense starts in month 1"),
        ("'P&L & Cash'!AL39", -5000, "Retainer continues during the forecast"),
        ("Team!C27", 0, "External counsel is not an employee"),
        ("Team!C31", 0, "External retainer does not trigger employee recruitment"),
        ("'P&L & Cash'!C12", 0, "No employee tools for fractional counsel"),
        ("'P&L & Cash'!C13", 0, "No employee workspace for fractional counsel"),
        ("'Employee AI'!C17", 0, "No employee AI expense for an external retainer"),
        ("Summary!B72", 0, "No employee AI allowance"),
        ("Summary!B74", 0, "No employee AI headroom"),
        ("Summary!B58", 120000, "Twenty-four monthly counsel payments"),
        ("Summary!B40", 120000, "Counsel is included in other operating cash uses"),
        ("Summary!B42", 120000, "External legal spending enters total cash uses once"),
        ("Summary!B25", 15000, "Three months of external legal operating buffer"),
        ("Summary!B22", 135000, "Funding covers legal spending and future buffer"),
        ("Summary!B27", 15000, "Month 24 cash equals operating buffer without AI earmark"),
    ], "The fictional retainer is 5000 local currency units in either region; FX is "
       "one. External counsel has no employee tools, hiring costs or AI allowance.")

    add("counsel_start_month", "Counsel has an explicit independent service start month", {
        **legal, "Assumptions!B127": 4, "Assumptions!B69": 2,
    }, [
        ("'P&L & Cash'!E39", 0, "No counsel expense before month 4"),
        ("'P&L & Cash'!F39", -5000, "Counsel starts in month 4"),
        ("'P&L & Cash'!G39", -5000, "Employee hiring delay does not shift external service timing"),
        ("Summary!B58", 105000, "Twenty-one monthly counsel payments through month 24"),
        ("Summary!B42", 105000, "Counsel cash uses follow the selected start"),
        ("Summary!B25", 15000, "Operating buffer retains the next three payments"),
        ("Summary!B22", 120000, "Funding covers twenty-four total payments including buffer"),
        ("Summary!B74", 0, "Service retainer does not create employee AI headroom"),
    ], "Counsel timing is a separate external-service input; the global employee "
       "hiring delay applies only to employees.")

    add("counsel_excluded", "External counsel inclusion control suppresses a positive retainer", {
        **legal, "Assumptions!B128": 0,
    }, [
        ("'P&L & Cash'!C39", 0, "No expense when counsel is excluded"),
        ("'P&L & Cash'!AL39", 0, "No later expense when counsel remains excluded"),
        ("Summary!B58", 0, "No external counsel cash use"),
        ("Summary!B42", 0, "No spending in the isolated company"),
        ("Summary!B25", 0, "No counsel operating buffer"),
        ("Summary!B22", 0, "No counsel funding requirement"),
    ], "A positive retainer and valid start month cannot bypass explicit exclusion.")

    add("legal_accounting_and_contingency", "Accounting and legal spending are distinct and reconcile once", {
        **legal, "Assumptions!B78": 2500, "Assumptions!B83": 0.05,
    }, [
        ("'P&L & Cash'!C16", -2500, "Accounting/payroll remains a separate operating line"),
        ("'P&L & Cash'!C39", -5000, "Counsel retainer is a separate operating line"),
        ("'P&L & Cash'!C20", -375, "Five-percent contingency applies to 7500 external costs"),
        ("'P&L & Cash'!C21", -7875, "No duplicate counsel inclusion in total operating expense"),
        ("'P&L & Cash'!C25", 7875, "Cash outflow equals accounting, counsel and contingency"),
        ("Summary!B58", 120000, "Counsel disclosure excludes accounting and contingency"),
        ("Summary!B40", 189000, "Twenty-four months of reconciled other operating expense"),
        ("Summary!B42", 189000, "Cash uses contain external legal costs once"),
        ("Summary!B25", 23625, "Operating buffer includes expected external-cost contingency"),
        ("Summary!B22", 212625, "Net funding equals cash uses plus operating buffer"),
    ], "The combined historical legal/accounting line is not stacked on top of "
       "counsel. The fixture explicitly sets accounting to 2500 and legal to 5000.")

    mixed = {**no_staff(), "Assumptions!B62": 0.5,
             "Assumptions!B68": 1, "Assumptions!B70": 1}
    for row, share in [(5, 0.4), (12, 0.6), (13, 0.2), (46, 0)]:
        mixed.update({f"Team!AQ{row}": 1, f"Team!AM{row}": 1,
                      f"Team!B{row}": 120000, f"Team!AN{row}": share})
    add("four_function_allocation", "Salary and AI reconcile across delivery, Product, GTM and G&A", mixed, [
        ("Team!C27", 4, "Four employees start in month 1"),
        ("Team!C24", 40000, "Four monthly salaries of 10000"),
        ("Team!C29", 15000, "Founder and solutions Product payroll after delivery ownership"),
        ("Team!C30", 9000, "Commercial operating payroll after delivery ownership"),
        ("Team!C33", 10000, "Assurance payroll belongs to G&A"),
        ("Costs!C30", 6000, "1.2 delivery FTE shares times 50% ownership times 10000"),
        ("'Employee AI'!C20", 1500, "Delivery receives 0.6 owned FTE times 2500"),
        ("'Employee AI'!C21", 3750, "Product AI follows 1.5 operating FTE"),
        ("'Employee AI'!C22", 2250, "GTM AI follows 0.9 operating FTE"),
        ("'Employee AI'!C29", 2500, "G&A receives one full employee AI allocation"),
        ("'Employee AI'!C23", 10000, "Four allocations exhaust four employee AI expenses"),
        ("'Employee AI'!C24", 0, "No AI allocation residual"),
        ("Costs!C34", 7500, "Cost of sales contains delivery salary and AI exactly once"),
        ("'P&L & Cash'!C38", -8500, "Product, GTM and G&A AI enter operating expense once"),
        ("'P&L & Cash'!C21", -42500, "Operating payroll 34000 plus employee AI 8500"),
        ("'P&L & Cash'!C25", 50000, "Total salary 40000 and AI 10000 exhaust monthly cash use"),
        ("Summary!B35", 360000, "Twenty-four months Product payroll"),
        ("Summary!B36", 216000, "Twenty-four months GTM payroll"),
        ("Summary!B37", 144000, "Twenty-four months delivery payroll"),
        ("Summary!B48", 240000, "Twenty-four months G&A payroll"),
        ("Summary!B47", 240000, "All functional AI spending appears once in cash uses"),
        ("Summary!B42", 1200000, "Gross uses reconcile to 24 times 50000"),
        ("Summary!B74", 240000, "Unused allowance stays outside gross cash uses"),
        ("Summary!B25", 150000, "Three months of recurring salary and AI expense"),
        ("Summary!B22", 1590000, "1200000 expenses plus 150000 buffer plus 240000 headroom"),
    ], "Each employee has a 120000 fictional salary and starts in month 1. Service "
       "ownership changes functional allocation; it does not change total cash cost.")

    for utilisation, spending, unused, funding, buffer in [
        (0, 0, 110000, 110000, 0),
        (1, 110000, 0, 125000, 15000),
    ]:
        add(f"ga_utilisation_{int(utilisation * 100)}", "G&A employee at an AI utilisation endpoint", {
            **assurance_employee(), "Assumptions!B120": utilisation,
        }, [
            ("'Employee AI'!E17", 5000 * utilisation, "Expected monthly AI expense"),
            ("'Employee AI'!E29", 5000 * utilisation, "G&A allocation matches expected expense"),
            ("'Employee AI'!E18", 5000 * (1 - utilisation), "Unspent allowance remains cash"),
            ("Summary!B73", spending, "22 employee-months expected spending"),
            ("Summary!B74", unused, "22 employee-months unspent allowance"),
            ("Summary!B42", spending, "Unused allowance is never an expense"),
            ("Summary!B25", buffer, "Following-quarter buffer follows expected spending"),
            ("Summary!B22", funding, "Funding includes spending, buffer and unused allowance once"),
            ("Summary!B27", unused + buffer, "Closing cash retains the calculated earmark and buffer"),
        ], "The month-3 assurance employee has zero salary multiplier only to "
           "isolate AI arithmetic at zero and full utilisation.")

    assert len(regressions) == 30
    return result, regressions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=ROOT)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    new_cases, regressions = build_cases()
    combined = regressions + new_cases
    assert len({case["id"] for case in combined}) == len(combined)
    for case in combined:
        assert case["checks"] and all(check["tolerance"] >= 0 for check in case["checks"])
    path = args.output_dir / "assurance-validation-cases.json"
    path.write_text(json.dumps(combined, indent=2) + "\n")
    print(f"{path}: {len(regressions)} regressions + {len(new_cases)} new cases; "
          f"{sum(len(case['checks']) for case in combined)} expected arithmetic checks")


if __name__ == "__main__":
    main()
