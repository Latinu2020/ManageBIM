// ManageBIM - exemplu de script (Rhai)
// Motorul de executie se activeaza in 0.28. API-ul de mai jos este cel planificat.
// Scop: selecteaza usile cu rezistenta la foc si afiseaza un mesaj in aplicatie.

let category = "Doors";
let prop = "FireRating";

// selectare dupa categorie, apoi doar cele care au proprietatea specificata
select_where("Category", category);
let doors = selected_count();
log("Usi in model: " + doors);

// restrange selectia la cele cu FiringRating = EI30
select_where(prop, "EI30");
let fire_doors = selected_count();
log("Usi EI30: " + fire_doors);

// evidentiere in model si zoom
zoom_to_selection();

// Rezultatul se vede in bara de selectie si in Properties.
