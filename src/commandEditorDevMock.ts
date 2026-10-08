// Browser-only emulation of the editor IPC. Production state is owned by Rust.
import type { AdvancedBattleScene, BattleScene, BattleTurn } from "./types/command";
import type { CommandDocument, CommandMutation } from "./features/battle/useCommandDocument";
import { createDefaultScene, normalizeScene } from "./features/advanced/advancedCommandModel";

type DocumentScene = BattleScene | AdvancedBattleScene;
type Bridge = (command: string, args: Record<string, unknown>) => Promise<unknown>;
function normalScene(): BattleScene {
  return {id:crypto.randomUUID(),turns:[{id:crypto.randomUUID(),preparationActions:[],servantActions:[],equipmentActions:[],commandSpellActions:[],enemyTarget:null,attackPriority:Array.from({length:3},()=>({id:crypto.randomUUID(),card:null})),attackMode:"normal",criticalStrategy:{memberPriority:[],chainPriority:["mighty","buster","arts","quick"]},advancedCardStrategy:{customRules:[]}}]};
}
function normalize(scenes: DocumentScene[], advanced: boolean): DocumentScene[] {
  if(advanced)return (scenes.length ? scenes : [createDefaultScene()]).map(scene=>normalizeScene(scene as AdvancedBattleScene));
  return (scenes.length ? scenes : [normalScene()]).map(value=>{
    const scene=value as BattleScene;
    return {id:scene.id,turns:(scene.turns?.length ? scene.turns : [{...scene,id:`${scene.id}_turn_1`} as BattleTurn]).map(turn=>{
      const cards=[...(turn.attackPriority??[])];while(cards.length<3)cards.push({id:crypto.randomUUID(),card:null});
      return {...turn,preparationActions:turn.preparationActions ?? [...(turn.servantActions??[]),...(turn.equipmentActions??[]),...(turn.commandSpellActions??[])],servantActions:[],equipmentActions:[],commandSpellActions:[],attackPriority:cards,attackMode:turn.attackMode??"normal"};
    })};
  });
}

export function commandEditorDevBridge(bridge: Bridge): Bridge {
  const histories=new Map<string,{before:DocumentScene[];wave:number;turn:number}>();
  const documents=new Map<string,DocumentScene[]>();
  return async(command,args)=>{
    if(command!=="load_command_editor"&&command!=="mutate_command_editor")return bridge(command,args);
    const advanced=args.advanced===true,key=`${args.projectId}:${advanced}`;
    const load=advanced?"load_advanced_battle_scenes":"load_battle_scenes",save=advanced?"save_advanced_battle_scenes":"save_battle_scenes";
    if(command==="load_command_editor"){
      const scenes=normalize((await bridge(load,{projectId:args.projectId}) as DocumentScene[] | null) ?? [],advanced);
      documents.set(key,scenes);
      return {scenes,wave:0,turn:0,canUndo:histories.has(key)};
    }
    const before=documents.get(key);if(!before)throw Error("请先加载指令");
    let scenes=structuredClone(before),wave=Number(args.wave),turn=Number(args.turn);
    const mutation=args.mutation as CommandMutation;
    if(mutation.type==="undo"){
      const history=histories.get(key);if(!history)throw Error("没有可撤销的修改");
      scenes=history.before;wave=history.wave;turn=history.turn;
    }else{
      if(mutation.type==="addWave"){if(advanced)throw Error("冠位模式只支持一面");scenes.push(normalScene());wave=scenes.length-1;turn=0;}
      else if(mutation.type==="deleteWave"){if(scenes.length<=1)throw Error("至少保留一面");scenes.splice(wave,1);wave=Math.min(wave,scenes.length-1);turn=0;}
      else if(mutation.type==="addTurn"){const scene=scenes[wave];if(advanced)(scene as AdvancedBattleScene).turns!.push(createDefaultScene().turns![0]);else(scene as BattleScene).turns.push(normalScene().turns[0]);turn=scene.turns!.length-1;}
      else if(mutation.type==="deleteTurn"){const turns=scenes[wave].turns!;if(turns.length<=1)throw Error("至少保留一个回合");turns.splice(turn,1);turn=Math.min(turn,turns.length-1);}
      else if(mutation.type==="updateScene")scenes[wave]=mutation.scene;
      else if(mutation.type==="updateTurn")(scenes[wave] as BattleScene).turns[turn]=mutation.turn;
    }
    await bridge(save,{projectId:args.projectId,scenes});documents.set(key,scenes);
    if(mutation.type === "undo") histories.delete(key);
    else histories.set(key,{before:structuredClone(before),wave:Number(args.wave),turn:Number(args.turn)});
    return {scenes,wave,turn,canUndo:histories.has(key)} satisfies CommandDocument<DocumentScene>;
  };
}
