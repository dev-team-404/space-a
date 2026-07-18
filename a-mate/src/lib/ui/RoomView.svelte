<script lang="ts">
  import { robotSpecForSeed, roomCapabilities, roomMoveCell, roomSaveDesign, roomView, type RoomMe, type RoomObject, type RoomState } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render'; import { frameAt } from '../robot/anim';
  import FurnitureSprite from './FurnitureSprite.svelte';
  import { CATEGORY_LABELS, FURNITURE, FURNITURE_BY_ID, ROTATIONS, THEMES, canonicalizeFurnitureGeometry, themeFor, wallRotation, type FurnitureCategory, type PlacedFurniture, type Rotation, type WallSide } from '../interior/catalog';
  import { ROOM_GRID, isInsideRoom, occupiedWorldCells, placementOrigin, rotatedOffsets, rotatedOrigin, rotatedSize, spriteGroundAnchor, wallOccupiedIndices, wallPlacementOrigin } from '../interior/geometry';
  const VW=900,VH=460,TW=28,TH=14,OX=450,OY=140,WALL=110;
  const GRID_W=ROOM_GRID.w,GRID_H=ROOM_GRID.h;
  const previewMode=import.meta.env.DEV&&new URLSearchParams(location.search).has('interiorPreview');
  let me=$state<RoomMe|null>(null),room=$state<RoomState|null>(null),draft=$state<RoomState['design']|null>(null);
  let editing=$state(false),saving=$state(false),error=$state(''),flash=$state(''),category=$state<FurnitureCategory>('sofa'),selectedId=$state(FURNITURE[0].id),rotation=$state<Rotation>(0),wall=$state<WallSide>('north');
  let hoverCell=$state<[number,number]|null>(null),menu=$state<{index:number;x:number;y:number}|null>(null);
  let hoverWall=$state<{side:'north'|'west';index:number}|null>(null);
  const activeDesign=$derived(editing&&draft?draft:room?.design??null), theme=$derived(activeDesign?themeFor(activeDesign.wallpaper,activeDesign.floor):THEMES[0]);
  const isOwner=$derived(!!room&&!!me&&room.room_id===me.my_room_id), categoryItems=$derived(FURNITURE.filter(i=>i.category===category));
  const categories=Object.keys(CATEGORY_LABELS) as FurnitureCategory[];
  const iso=(x:number,y:number):[number,number]=>[OX+(x-y)*TW/2,OY+(x+y)*TH/2];
  const a=$derived(iso(0,0)),b=$derived(iso(GRID_W,0)),c=$derived(iso(GRID_W,GRID_H)),d=$derived(iso(0,GRID_H));
  const points=(p:[number,number][])=>p.map(v=>v.join(',')).join(' ');
  function tile(x:number,y:number){return points([iso(x,y),iso(x+1,y),iso(x+1,y+1),iso(x,y+1)]);}
  function wallTile(side:'north'|'west',i:number){const p1=side==='north'?iso(i,0):iso(0,i),p2=side==='north'?iso(i+1,0):iso(0,i+1);return points([[p1[0],p1[1]-WALL],[p2[0],p2[1]-WALL],p2,p1]);}
  if(previewMode){
    me={agent_id:'preview-owner',name:'준녕',my_room_id:'preview-room',room_id:'preview-room',cell:[12,10]};
    room={room_id:'preview-room',owner_name:'준녕',owner_mascot_seed:'preview-owner',grid:{w:GRID_W,h:GRID_H},design:{wallpaper:'lavender-dream',floor:'lavender-dream-floor',objects:[
      {asset_id:'window.navy-wide',category:'window',cell:[10,0],size:[5,2],rotation:180,wall:'north'},
    ]},occupants:[
      {agent_id:'preview-owner',name:'준녕',cell:[12,10],is_owner:true,mascot_seed:'preview-owner'},
      {agent_id:'preview-guest',name:'유연',cell:[18,17],is_owner:false,mascot_seed:'preview-guest'},
    ]};
  }
  async function poll(){if(previewMode||editing)return;try{const v=await roomView();me=v.me;room={...v.room,design:normalizedDesign(v.room.design)};error='';}catch(e){error=`${e}`}}
  poll(); $effect(()=>{const t=setInterval(poll,2000);return()=>clearInterval(t)});
  function msg(s:string){flash=s;setTimeout(()=>{if(flash===s)flash=''},2200)}
  function cells(o:Pick<RoomObject,'cell'|'size'|'rotation'|'footprint'>){return occupiedWorldCells(o).map(([x,y])=>`${x},${y}`)}
  const occupied=$derived.by(()=>{const s=new Set<string>();if(room)room.occupants.forEach(o=>s.add(`${o.cell[0]},${o.cell[1]}`));activeDesign?.objects.filter(o=>o.category!=='window').forEach(o=>cells(o).forEach(c=>s.add(c)));return s});
  function canPlace(o:PlacedFurniture,ignore=-1){if(!room||!draft)return false;if(o.category==='window'){if(!o.wall||o.rotation!==wallRotation(o.wall))return false;const limit=o.wall==='west'?GRID_H:GRID_W;if(o.cell[0]<0||o.cell[0]+o.size[0]>limit)return false;const cells=new Set(wallOccupiedIndices(o.cell[0],o.size[0]));return !draft.objects.some((v,i)=>i!==ignore&&v.category==='window'&&v.wall===o.wall&&wallOccupiedIndices(v.cell[0],v.size[0]).some(index=>cells.has(index)))}
    if(!isInsideRoom(o))return false;const blocked=new Set(room.occupants.map(v=>`${v.cell[0]},${v.cell[1]}`));draft.objects.forEach((v,i)=>{if(i!==ignore&&v.category!=='window')cells(v).forEach(c=>blocked.add(c))});return cells(o).every(c=>!blocked.has(c))}
  function candidateAt(x:number,y:number):PlacedFurniture|null{const item=FURNITURE_BY_ID.get(selectedId);if(!item)return null;return{asset_id:item.id,category:item.category,cell:placementOrigin([x,y],item.size,rotation),size:item.size,footprint:item.footprint,rotation,wall:item.category==='window'?wall:null}}
  function windowCandidate(index:number,side:WallSide):PlacedFurniture|null{const item=FURNITURE_BY_ID.get(selectedId);if(!item||item.category!=='window')return null;const limit=side==='west'?GRID_H:GRID_W;return{asset_id:item.id,category:item.category,cell:[wallPlacementOrigin(index,item.size[0],limit),0],size:item.size,footprint:item.footprint,rotation:wallRotation(side),wall:side}}
  const ghost=$derived.by(()=>{if(!editing)return null;if(category==='window'&&hoverWall)return windowCandidate(hoverWall.index,hoverWall.side);return hoverCell?candidateAt(hoverCell[0],hoverCell[1]):null});
  function place(x:number,y:number){const o=candidateAt(x,y);if(!o||!draft)return;if(!canPlace(o))return msg('표시된 점유 셀에 다른 가구나 캐릭터가 있습니다.');draft.objects=[...draft.objects,o];selectedId='';hoverCell=null}
  async function clickFloor(x:number,y:number){if(editing){if(category!=='window')place(x,y);return}if(occupied.has(`${x},${y}`))return;try{await roomMoveCell(x,y);await poll()}catch{msg('이동할 수 없는 자리예요.')}}
  function hoverWindow(index:number,side:WallSide){wall=side;rotation=wallRotation(side);hoverWall={side,index}}
  function placeWindow(index:number,side:WallSide){wall=side;rotation=wallRotation(side);if(editing&&category==='window'){const o=windowCandidate(index,side);if(!o||!draft)return;if(!canPlace(o))return msg('표시된 벽 셀에 다른 창문이 있습니다.');draft.objects=[...draft.objects,o];selectedId='';hoverWall=null}}
  function rotatePlaced(i:number,step:1|-1){if(!draft)return;const current=draft.objects[i];if(current.category==='window')return;const at=ROTATIONS.indexOf(current.rotation),next=ROTATIONS[(at+step+4)%4],cell=rotatedOrigin(current,next),o={...current,cell,rotation:next};if(canPlace(o as PlacedFurniture,i)){draft.objects=draft.objects.map((v,n)=>n===i?o:v);menu=null}else msg('회전한 점유 셀이 다른 가구나 캐릭터와 겹칩니다.')}
  function removePlaced(i:number){if(draft)draft.objects=draft.objects.filter((_,n)=>n!==i);menu=null}
  function openMenu(e:MouseEvent,index:number){if(!editing)return;e.preventDefault();const host=(e.currentTarget as HTMLElement).closest('.scene')!.getBoundingClientRect();menu={index,x:e.clientX-host.left,y:e.clientY-host.top}}
  async function assertProtocol(){if(previewMode)return true;try{const c=await roomCapabilities();if(c.room_protocol===3&&c.grid.w===GRID_W&&c.grid.h===GRID_H&&c.floor_min_y===0&&c.footprint_mask)return true}catch{}msg('룸 서버가 구버전입니다. 새 room-server를 재시작해야 인테리어를 저장할 수 있습니다.');return false}
  function normalizedDesign(design:RoomState['design']){const next:RoomState['design']=JSON.parse(JSON.stringify(design));next.objects=next.objects.filter(o=>o.category!=='dining').map(o=>{const canonical=canonicalizeFurnitureGeometry(o);return canonical.category==='window'&&(canonical.wall==='north'||canonical.wall==='west')?{...canonical,rotation:wallRotation(canonical.wall)}:canonical});return next}
  async function beginEdit(){if(room&&isOwner&&await assertProtocol()){draft=normalizedDesign(room.design);editing=true}}
  function chooseCategory(c:FurnitureCategory){category=c;selectedId=FURNITURE.find(i=>i.category===c)!.id;rotation=c==='window'?wallRotation(wall):0}
  function chooseTheme(id:string,floor:string){if(draft){draft.wallpaper=id;draft.floor=floor}}
  function cancel(){editing=false;draft=null}
  async function save(){if(!room||!draft||!await assertProtocol())return;saving=true;try{const saved=await roomSaveDesign(room.room_id,draft);room={...saved,design:normalizedDesign(saved.design)};editing=false;draft=null;msg('인테리어를 저장했어요.')}catch(e){msg(`저장 실패: ${e}`)}finally{saving=false}}
  const specs=new Map<string,RobotSpec>(); function robot(node:HTMLCanvasElement,name:string){let raf=0;const run=(spec:RobotSpec)=>{const c=node.getContext('2d')!;const loop=(t:number)=>{drawRobot(c,spec,frameAt('idle',t));raf=requestAnimationFrame(loop)};loop(0)};specs.has(name)?run(specs.get(name)!):robotSpecForSeed(name).then(s=>{specs.set(name,s);run(s)});return{destroy:()=>cancelAnimationFrame(raf)}}
  function spriteRotation(o:RoomObject|PlacedFurniture):Rotation{return o.category==='window'&&(o.wall==='north'||o.wall==='west')?wallRotation(o.wall):o.rotation}
  function objectLayout(o:RoomObject){
    const item=FURNITURE_BY_ID.get(o.asset_id),visualRotation=spriteRotation(o),imageAnchor=item?.render.anchors[visualRotation]??[.5,1] as [number,number];
    if(o.category==='window'){const side=o.wall??'north',i=o.cell[0]+o.size[0]/2,p=side==='west'?iso(0,i):iso(i,0),width=Math.max(62,o.size[0]*TW*.78);return{x:p[0],y:p[1]-WALL*.3,z:5,w:width,anchor:imageAnchor}}
    const [w,h]=rotatedSize(o.size,o.rotation),ground=spriteGroundAnchor(o,item?.render.footprintAnchor),p=iso(ground[0],ground[1]),width=(item?.render.widthTiles??(w+h)/2)*TW;
    return{x:p[0],y:p[1],z:20+Math.round((ground[0]+ground[1])*10),w:width,anchor:imageAnchor};
  }
</script>

{#if room&&me&&activeDesign}
<div class="roomwrap"><div class="title"><b>{room.owner_name}님의 방</b>{#if me.room_id!==me.my_room_id}<span>방문 중</span>{/if}<small>{room.occupants.length}명</small>{#if flash}<em>{flash}</em>{/if}{#if isOwner&&!editing}<button onclick={beginEdit}>인테리어 설정</button>{/if}</div>
  <div class="scene">
    <svg viewBox={`0 0 ${VW} ${VH}`} aria-label="아이소메트릭 미니룸">
      <polygon class="north" fill={theme.wall} points={points([[a[0],a[1]-WALL],[b[0],b[1]-WALL],b,a])}/>
      <polygon class="west" fill={theme.wallSide} points={points([[a[0],a[1]-WALL],a,d,[d[0],d[1]-WALL]])}/>
      <polygon fill={theme.floorBase} points={points([a,b,c,d])}/>
      {#each Array(GRID_H) as _,y}{#each Array(GRID_W) as _,x}<polygon class="tile" class:taken={occupied.has(`${x},${y}`)} fill={(x+y)%2?theme.floorAlt:theme.floorBase} stroke={theme.grout} points={tile(x,y)} onmouseenter={()=>hoverCell=[x,y]} onclick={()=>clickFloor(x,y)}/>{/each}{/each}
      {#if ghost&&ghost.category!=='window'}
        {#each rotatedOffsets(ghost) as offset}
          <polygon class="ghost-cell" class:blocked={!canPlace(ghost)} points={tile(ghost.cell[0]+offset[0],ghost.cell[1]+offset[1])}/>
        {/each}
      {/if}
      {#if editing}
        {#each activeDesign.objects as placed,placedIndex (`footprint-${placedIndex}`)}
          {#if placed.category!=='window'}
            {#each rotatedOffsets(placed) as offset}
              <polygon class="placed-cell" points={tile(placed.cell[0]+offset[0],placed.cell[1]+offset[1])}/>
            {/each}
            {@const groundPoint=spriteGroundAnchor(placed,FURNITURE_BY_ID.get(placed.asset_id)?.render.footprintAnchor)}
            {@const groundPixel=iso(groundPoint[0],groundPoint[1])}
            <circle class="ground-point" cx={groundPixel[0]} cy={groundPixel[1]} r="2.4"/>
          {/if}
        {/each}
      {/if}
      {#if editing&&category==='window'}
        {#each Array(GRID_W) as _,x}<polygon class="wallslot" class:hovered={hoverWall?.side==='north'&&hoverWall.index===x} points={wallTile('north',x)} onmouseenter={()=>hoverWindow(x,'north')} onclick={()=>placeWindow(x,'north')}/>{/each}
        {#each Array(GRID_H) as _,y}<polygon class="wallslot" class:hovered={hoverWall?.side==='west'&&hoverWall.index===y} points={wallTile('west',y)} onmouseenter={()=>hoverWindow(y,'west')} onclick={()=>placeWindow(y,'west')}/>{/each}
      {/if}
    </svg>
    {#if ghost}{@const gp=objectLayout(ghost)}<div class="furniture ghost" class:invalid={!canPlace(ghost)} style:left={`${gp.x/VW*100}%`} style:top={`${gp.y/VH*100}%`} style:width={`${gp.w/VW*100}%`} style:transform={`translate(${-gp.anchor[0]*100}%,${-gp.anchor[1]*100}%)`} style:z-index={gp.z}><FurnitureSprite assetId={ghost.asset_id} rotation={spriteRotation(ghost)} room/></div>{/if}
    {#each activeDesign.objects as o,i (`${o.asset_id}-${i}`)}{@const p=objectLayout(o)}<button class="furniture placed" data-category={o.category} data-object-index={i} style:left={`${p.x/VW*100}%`} style:top={`${p.y/VH*100}%`} style:width={`${p.w/VW*100}%`} style:transform={`translate(${-p.anchor[0]*100}%,${-p.anchor[1]*100}%)`} style:z-index={p.z} oncontextmenu={e=>openMenu(e,i)}><FurnitureSprite assetId={o.asset_id} rotation={spriteRotation(o)} label={FURNITURE_BY_ID.get(o.asset_id)?.name} room/></button>{/each}
    {#each room.occupants as o (o.agent_id)}{@const p=iso(o.cell[0]+.5,o.cell[1]+.5)}<div class="agent" style:left={`${p[0]/VW*100}%`} style:top={`${p[1]/VH*100}%`} style:z-index={30+Math.round((o.cell[0]+o.cell[1])*10)}><canvas width="128" height="128" use:robot={o.mascot_seed||o.name}></canvas><span>{o.name}</span></div>{/each}
    {#if menu}{@const menuObject=activeDesign.objects[menu.index]}<div class="object-menu" style:left={`${menu.x}px`} style:top={`${menu.y}px`}>{#if menuObject?.category!=='window'}<button onclick={()=>rotatePlaced(menu!.index,1)}>정방향 90° 회전</button><button onclick={()=>rotatePlaced(menu!.index,-1)}>역방향 90° 회전</button>{/if}<button class="remove" onclick={()=>removePlaced(menu!.index)}>가구 배치 해제</button></div><button class="menu-dismiss" aria-label="메뉴 닫기" onclick={()=>menu=null}></button>{/if}
  </div>
  {#if editing&&draft}<section class="editor"><header><div><b>인테리어 설정</b><small>바닥 가구는 우클릭으로 회전·해제하고, 창문은 벽 방향에 고정해 배치합니다.</small></div><nav><button onclick={cancel}>취소</button><button class="primary" onclick={save} disabled={saving}>{saving?'저장 중…':'저장'}</button></nav></header>
    <div class="themes">{#each THEMES as t}<button class:active={draft.wallpaper===t.wallpaper} onclick={()=>chooseTheme(t.wallpaper,t.floor)}><i style:background={t.wall}></i><i style:background={t.floorBase}></i><small>{t.name}</small></button>{/each}</div>
    <div class="tabs">{#each categories as c}<button class:active={category===c} onclick={()=>chooseCategory(c)}>{CATEGORY_LABELS[c]}</button>{/each}</div>
    <div class="catalog">{#each categoryItems as item}<button class:active={selectedId===item.id} onclick={()=>selectedId=item.id}><span><FurnitureSprite assetId={item.id} rotation={rotation} label={item.name}/></span><small>{item.name}</small></button>{/each}</div>
    <div class="directions">{#if category==='window'}<span>창문 방향은 벽에 고정됩니다: 좌상벽 ↘ · 우상벽 ↙</span>{:else}방향 {#each ROTATIONS as r}<button class:active={rotation===r} onclick={()=>rotation=r}>{['↗','↘','↙','↖'][ROTATIONS.indexOf(r)]}</button>{/each}{/if}</div>
  </section>{/if}
</div>
{:else if error}<div class="empty">방 서버에 연결하지 못했습니다. 설정에서 Room Server 연결을 확인하세요.</div>{/if}

<style>
  .roomwrap{display:flex;flex-direction:column;gap:8px}.title{display:flex;align-items:center;gap:8px;font-size:12px}.title span{background:#ffd9ca;border-radius:99px;padding:2px 8px}.title small{color:var(--ink-soft)}.title em{font-style:normal;color:var(--accent)}button{font:inherit;color:inherit}.title button{margin-left:auto;border:0;border-radius:99px;padding:5px 11px;background:var(--accent);color:white;cursor:pointer}
  .scene{position:relative;width:min(100%,900px);aspect-ratio:900/460;margin:auto;overflow:hidden;border-radius:14px;background:linear-gradient(#fbf8ee,#e9e4d8);box-shadow:var(--shadow-soft)}svg{position:absolute;inset:0;width:100%;height:100%}.north,.west{stroke:#a69483;stroke-width:2}.tile{cursor:pointer;stroke-width:.65;transition:filter .1s}.tile:hover{filter:brightness(1.12)}.tile.taken{cursor:not-allowed}.ghost-cell{fill:rgba(68,211,159,.38);stroke:#24a77a;stroke-width:1.2;pointer-events:none}.ghost-cell.blocked{fill:rgba(226,80,80,.4);stroke:#c93636}.placed-cell{fill:rgba(44,166,224,.08);stroke:#2ca6e0;stroke-width:.85;pointer-events:none}.ground-point{fill:#ff5b5b;stroke:white;stroke-width:.8;pointer-events:none}.wallslot{fill:rgba(116,225,197,.06);stroke:rgba(124,111,208,.22);stroke-width:.7;cursor:crosshair;transition:fill .1s}.wallslot:hover,.wallslot.hovered{fill:rgba(116,225,197,.38);stroke:#58cdb0;stroke-width:1.5}
  .furniture{position:absolute;height:auto;min-width:0!important;max-width:none;overflow:visible;border:0;padding:0;background:transparent;pointer-events:none;filter:drop-shadow(0 2px 1px #544b4533)}.furniture.placed{pointer-events:auto;cursor:default}.ghost{opacity:.48}.ghost.invalid{opacity:.25;filter:grayscale(1) drop-shadow(0 0 3px #d33)}.agent{position:absolute;width:7%;height:14%;transform:translate(-50%,-82%);pointer-events:none}.agent canvas{width:100%;height:100%;image-rendering:pixelated}.agent span{position:absolute;left:50%;bottom:-3px;transform:translateX(-50%);white-space:nowrap;background:#fffaf0;border-radius:99px;padding:1px 6px;font-size:9px}.object-menu{position:absolute;z-index:1001;display:flex;flex-direction:column;min-width:145px;padding:5px;color:#443b35;background:#fffdf8;border:1px solid #c8bea9;border-radius:8px;box-shadow:0 6px 20px #342d2440}.object-menu button{border:0;padding:7px 9px;text-align:left;background:transparent;border-radius:5px;cursor:pointer}.object-menu button:hover{background:var(--pastel-lavender)}.object-menu .remove{color:#b74444}.menu-dismiss{position:absolute;inset:0;z-index:1000;border:0;background:transparent}
  .editor{display:flex;flex-direction:column;gap:9px;padding:12px;background:var(--frame-bg);border-radius:12px;box-shadow:var(--shadow-soft)}header{display:flex;justify-content:space-between;gap:10px}header div{display:flex;flex-direction:column}header small{color:var(--ink-soft)}nav{display:flex;gap:6px}nav button,.tabs button,.directions button{border:0;border-radius:7px;padding:5px 9px;background:var(--pastel-lavender);cursor:pointer}.primary,.tabs button.active,.directions button.active{background:var(--accent)!important;color:white}.themes{display:grid;grid-template-columns:repeat(10,1fr);gap:5px}.themes button{height:52px;position:relative;display:grid;grid-template-rows:1fr 1fr;border:2px solid transparent;border-radius:7px;overflow:hidden;padding:0}.themes button.active,.catalog button.active{border-color:var(--accent)}.themes i{display:block}.themes small{position:absolute;inset:auto 1px 1px;background:#ffffffcc;font-size:8px}.tabs{display:flex;flex-wrap:wrap;gap:5px}.catalog{display:grid;grid-template-columns:repeat(5,1fr);gap:6px}.catalog button{height:104px;min-width:0;display:grid;grid-template-rows:80px auto;border:2px solid transparent;border-radius:8px;background:#fffdfa;overflow:hidden}.catalog span{display:block;min-width:0;min-height:0;overflow:hidden;padding:5px}.catalog span :global(canvas){display:block!important;width:100%!important;height:100%!important;min-width:0!important;max-width:100%!important}.catalog small{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.directions{display:flex;align-items:center;gap:5px;font-size:11px}.empty{padding:14px;background:var(--frame-bg);border-radius:12px;color:var(--ink-soft)}
</style>
