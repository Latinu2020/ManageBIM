// ManageBIM - exemplu de script (Rhai) - setup rapid Clash Detective
// Creeaza seturile de teste uzuale pentru coordonarea arhitectura / structura / MEP.

let pairs = [
    ["Architecture vs Structure", "Walls", "Columns"],
    ["Architecture vs HVAC", "Walls", "Ducts"],
    ["Structure vs Piping", "Beams", "Pipes"],
    ["Ceiling vs Sprinklers", "Coverings", "Sprinklers"],
];

for p in pairs {
    let name = p[0];
    let set_a = select_where("Category", p[1]);
    let set_b = select_where("Category", p[2]);
    create_clash_test(name, 25.0);
    log("Test creat: " + name);
}

log("4 teste de clash pregatite. Ruleaza Update All in Clash Detective.");
