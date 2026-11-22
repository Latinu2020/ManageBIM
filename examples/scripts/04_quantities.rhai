// Cantitati rapide pe categorii - inlocuieste selectia cu elementele fiecarei categorii.
// Foloseste: apasa Run, apoi Export > Properties / Quantities din aplicatie.
let cats = ["Walls", "IfcWall", "Columns", "IfcColumn", "Beams", "IfcBeam",
            "Doors", "IfcDoor", "Windows", "IfcWindow", "Slabs", "IfcSlab"];

for cat in cats {
    let n = select_where("Category", cat);
    if n > 0 {
        log(cat + ": " + n.to_string() + " elemente");
    }
}
select_ids([]);
log("Selectia a fost golita. Ruleaza cantitatile din panoul QTO.");
