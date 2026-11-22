// ManageBIM - exemplu de script (Rhai) - curatire date IFC
// Gaseste elementele fara clasificare si le marcheaza cu o proprietate
// de utilizator, ca sa fie usor de gasit ulterior in Find Items.

let unchecked = find_where_empty("Classification");
log("Elemente fara clasificare: " + unchecked.len());

for id in unchecked {
    select_ids([id]);
    set_prop("NeedsReview", "yes");
    set_prop("ReviewReason", "missing classification");
}

// Selectia finala = toate elementele marcate, ca sa le vezi in model
select_where("NeedsReview", "yes");
zoom_to_selection();
log("Marcate " + unchecked.len() + " elemente. Exporta un raport cu export_excel(\"review.xlsx\").");
