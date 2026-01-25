// Validate that the project name ends with -battery-pack
let name = variable::get("project-name");

if !name.ends_with("-battery-pack") {
    abort("Battery pack names must end with '-battery-pack'. Got: " + name);
}
