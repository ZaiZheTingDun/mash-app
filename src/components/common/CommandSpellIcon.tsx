import maleIcon from "../../../src-tauri/resources/images/commandspell/commandspell_male.png";
import femaleIcon from "../../../src-tauri/resources/images/commandspell/commandspell_female.png";
import type { MysticCodeGender } from "../../types/appUiSettings";

export function CommandSpellIcon({ gender = "female", label = "" }: {
  gender?: MysticCodeGender;
  label?: string;
}) {
  return <img className="command-spell-icon" src={gender === "male" ? maleIcon : femaleIcon} alt={label} draggable={false} />;
}
