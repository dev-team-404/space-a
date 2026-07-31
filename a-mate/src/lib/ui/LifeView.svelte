<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { tick } from 'svelte';
  import { getSprite, getOccupantSprite, requestOccupantSprite, robotSpecForSeed, lifeCapabilities, lifeMascotImage, lifeMoveCell, lifeSaveDesign, lifeSyncMascotImage, lifeView, type LifeMe, type LifeObject, type LifeState } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render'; import { frameAt } from '../robot/anim';
  import FurnitureSprite from './FurnitureSprite.svelte';
  import NameChip from './NameChip.svelte';
  import { CATEGORY_LABELS, FURNITURE, FURNITURE_BY_ID, ROTATIONS, THEMES, canonicalizeFurnitureGeometry, themeFor, wallRotation, type FurnitureCategory, type PlacedFurniture, type Rotation, type WallSide } from '../interior/catalog';
  import { LIFE_FLOOR, LIFE_GRID, isFloorCell, isInsideLife, occupiedWorldCells, placementOrigin, rotatedOffsets, rotatedOrigin, rotatedSize, spriteGroundAnchor, wallOccupiedIndices, wallPlacementOrigin, wallSpanScreenWidth } from '../interior/geometry';
  import { lifeRenderSignatures, type LifeRenderSignatures, type LifeViewPayload } from '../interior/life-render-state';
  const VW=900,VH=460,TW=28,TH=14,OX=450,OY=140,WALL=110;
  // 실제 오브젝트가 없는 좌우 여백을 적극적으로 덜어낸 표시 뷰포트. 원래 좌표계는
  // 유지해 배치 계산을 건드리지 않고, 기본 창에서 라이프 공간이 더 크게 차도록 한다.
  // Crop more of the unused horizontal canvas so the life fills the default
  // window while preserving one uniform isometric scale in both axes.
  const VIEW_X=170,VIEW_Y=30,VIEW_W=560,VIEW_H=257;
  const GRID_W=LIFE_GRID.w,GRID_H=LIFE_GRID.h;
  const previewMode=import.meta.env.DEV&&new URLSearchParams(location.search).has('interiorPreview');
  let me=$state<LifeMe|null>(null),life=$state<LifeState|null>(null),draft=$state<LifeState['design']|null>(null);
  let root=$state<HTMLElement|null>(null),scene=$state<HTMLElement|null>(null),menuElement=$state<HTMLElement|null>(null);
  let editing=$state(false),saving=$state(false),moving=$state(false),error=$state(''),flash=$state(''),editorTab=$state<'design'|FurnitureCategory>('design'),themePage=$state(0),furniturePage=$state(0),category=$state<FurnitureCategory>('sofa'),selectedId=$state(''),rotation=$state<Rotation>(0),wall=$state<WallSide>('north');
  let hoverCell=$state<[number,number]|null>(null),menu=$state<{index:number;x:number;y:number}|null>(null);
  let hoverWall=$state<{side:'north'|'west';index:number}|null>(null);
  const activeDesign=$derived(editing&&draft?draft:life?.design??null), theme=$derived(activeDesign?themeFor(activeDesign.wallpaper,activeDesign.floor):THEMES[0]);
  const isOwner=$derived(!!life&&!!me&&life.life_id===me.my_life_id), categoryItems=$derived(FURNITURE.filter(i=>i.category===category));
  const themePageCount=Math.ceil(THEMES.length/5),themeItems=$derived(THEMES.slice(themePage*5,themePage*5+5));
  const furniturePageCount=$derived(Math.ceil(categoryItems.length/4)),furnitureItems=$derived(categoryItems.slice(furniturePage*4,furniturePage*4+4));
  const categories=Object.keys(CATEGORY_LABELS) as FurnitureCategory[];
  const iso=(x:number,y:number):[number,number]=>[OX+(x-y)*TW/2,OY+(x+y)*TH/2];
  const a=$derived(iso(0,0)),b=$derived(iso(GRID_W,0)),d=$derived(iso(0,GRID_H));
  const points=(p:[number,number][])=>p.map(v=>v.join(',')).join(' ');
  function tile(x:number,y:number){return points([iso(x,y),iso(x+1,y),iso(x+1,y+1),iso(x,y+1)]);}
  function wallTile(side:'north'|'west',i:number){const p1=side==='north'?iso(i,0):iso(0,i),p2=side==='north'?iso(i+1,0):iso(0,i+1);return points([[p1[0],VIEW_Y],[p2[0],VIEW_Y],p2,p1]);}
  if(previewMode){
    me={agent_id:'preview-owner',name:'준녕',my_life_id:'preview-life',life_id:'preview-life',cell:[12,10]};
    life={life_id:'preview-life',owner_agent_id:'preview-owner',owner_name:'준녕',owner_mascot_seed:'preview-owner',grid:{w:GRID_W,h:GRID_H},design:{wallpaper:'lavender-dream',floor:'lavender-dream-floor',objects:[
      {asset_id:'window.navy-wide',category:'window',cell:[10,0],size:[5,2],rotation:180,wall:'north'},
    ]},occupants:[
      {agent_id:'preview-owner',name:'준녕',cell:[9,8],is_owner:true,mascot_seed:'preview-owner'},
      {agent_id:'preview-guest',name:'유연',cell:[7,10],is_owner:false,mascot_seed:'preview-guest'},
    ]};
  }
  let scrolling=false,polling=false,pendingView:LifeViewPayload|null=null,lastSignatures:LifeRenderSignatures|null=null,scrollTimer:ReturnType<typeof setTimeout>|null=null;
  function applyView(v:LifeViewPayload){
    error='';
    const next=lifeRenderSignatures(v),previous=lastSignatures;
    const meChanged=!previous||next.me!==previous.me;
    const metaChanged=!previous||next.meta!==previous.meta;
    const designChanged=!previous||next.design!==previous.design;
    const occupantsChanged=!previous||next.occupants!==previous.occupants;
    if(meChanged)me=v.me;
    if(!life||metaChanged||designChanged||occupantsChanged){
      const design=designChanged||!life?normalizedDesign(v.life.design):life.design;
      const occupants=occupantsChanged||!life?v.life.occupants:life.occupants;
      life={...v.life,design,occupants};
    }
    lastSignatures=next;
  }
  async function poll(){if(previewMode||editing||polling)return;polling=true;try{const v=await lifeView();if(scrolling)pendingView=v;else applyView(v)}catch(e){error=`${e}`}finally{polling=false}}
  poll(); $effect(()=>{const t=setInterval(poll,2000);return()=>clearInterval(t)});
  $effect(()=>{const host=root?.closest('.content');if(!host)return;const onScroll=()=>{scrolling=true;if(scrollTimer)clearTimeout(scrollTimer);scrollTimer=setTimeout(()=>{scrolling=false;if(pendingView){const next=pendingView;pendingView=null;applyView(next)}},140)};host.addEventListener('scroll',onScroll,{passive:true});return()=>{host.removeEventListener('scroll',onScroll);if(scrollTimer)clearTimeout(scrollTimer)}});
  function msg(s:string){flash=s;setTimeout(()=>{if(flash===s)flash=''},2200)}
  function cells(o:Pick<LifeObject,'cell'|'size'|'rotation'|'footprint'>){return occupiedWorldCells(o).map(([x,y])=>`${x},${y}`)}
  const occupied=$derived.by(()=>{const s=new Set<string>();if(life)life.occupants.forEach(o=>s.add(`${o.cell[0]},${o.cell[1]}`));activeDesign?.objects.filter(o=>o.category!=='window').forEach(o=>cells(o).forEach(c=>s.add(c)));return s});
  function canPlace(o:PlacedFurniture,ignore=-1){if(!life||!draft)return false;if(o.category==='window'){if(!o.wall||o.rotation!==wallRotation(o.wall))return false;const limit=o.wall==='west'?GRID_H:GRID_W;if(o.cell[0]<0||o.cell[0]+o.size[0]>limit)return false;const cells=new Set(wallOccupiedIndices(o.cell[0],o.size[0]));return !draft.objects.some((v,i)=>i!==ignore&&v.category==='window'&&v.wall===o.wall&&wallOccupiedIndices(v.cell[0],v.size[0]).some(index=>cells.has(index)))}
    if(!isInsideLife(o))return false;const blocked=new Set(life.occupants.map(v=>`${v.cell[0]},${v.cell[1]}`));draft.objects.forEach((v,i)=>{if(i!==ignore&&v.category!=='window')cells(v).forEach(c=>blocked.add(c))});return cells(o).every(c=>!blocked.has(c))}
  function candidateAt(x:number,y:number):PlacedFurniture|null{const item=FURNITURE_BY_ID.get(selectedId);if(!item)return null;return{asset_id:item.id,category:item.category,cell:placementOrigin([x,y],item.size,rotation,LIFE_GRID,item.footprint),size:item.size,footprint:item.footprint,rotation,wall:item.category==='window'?wall:null}}
  function windowCandidate(index:number,side:WallSide):PlacedFurniture|null{const item=FURNITURE_BY_ID.get(selectedId);if(!item||item.category!=='window')return null;const limit=side==='west'?GRID_H:GRID_W;return{asset_id:item.id,category:item.category,cell:[wallPlacementOrigin(index,item.size[0],limit),0],size:item.size,footprint:item.footprint,rotation:wallRotation(side),wall:side}}
  const ghost=$derived.by(()=>{if(!editing)return null;if(category==='window'&&hoverWall)return windowCandidate(hoverWall.index,hoverWall.side);return hoverCell?candidateAt(hoverCell[0],hoverCell[1]):null});
  function place(x:number,y:number){const o=candidateAt(x,y);if(!o||!draft)return;if(!canPlace(o))return msg('표시된 점유 셀에 다른 가구나 캐릭터가 있습니다.');draft.objects=[...draft.objects,o];selectedId='';hoverCell=null}
  async function clickFloor(x:number,y:number){if(editing){if(editorTab!=='design'&&category!=='window')place(x,y);return}if(moving||occupied.has(`${x},${y}`))return;moving=true;try{const moved=await lifeMoveCell(x,y);me=moved;if(life)life={...life,occupants:life.occupants.map(o=>o.agent_id===moved.agent_id?{...o,cell:moved.cell}:o)};await poll()}catch{msg('이동할 수 없는 자리예요.')}finally{moving=false}}
  function hoverWindow(index:number,side:WallSide){wall=side;rotation=wallRotation(side);hoverWall={side,index}}
  function placeWindow(index:number,side:WallSide){wall=side;rotation=wallRotation(side);if(editing&&editorTab==='window'){const o=windowCandidate(index,side);if(!o||!draft)return;if(!canPlace(o))return msg('표시된 벽 셀에 다른 창문이 있습니다.');draft.objects=[...draft.objects,o];selectedId='';hoverWall=null}}
  function rotatePlaced(i:number,step:1|-1){if(!draft)return;const current=draft.objects[i];if(current.category==='window')return;const at=ROTATIONS.indexOf(current.rotation),next=ROTATIONS[(at+step+4)%4],cell=rotatedOrigin(current,next),o={...current,cell,rotation:next};if(canPlace(o as PlacedFurniture,i)){draft.objects=draft.objects.map((v,n)=>n===i?o:v);menu=null}else msg('회전한 점유 셀이 다른 가구나 캐릭터와 겹칩니다.')}
  function removePlaced(i:number){if(draft)draft.objects=draft.objects.filter((_,n)=>n!==i);menu=null}
  async function openMenu(e:MouseEvent,index:number){if(!editing||!scene)return;e.preventDefault();const host=scene.getBoundingClientRect();menu={index,x:e.clientX-host.left,y:e.clientY-host.top};await tick();if(!menu||menu.index!==index||!menuElement)return;const gutter=12;menu={...menu,x:Math.max(gutter,Math.min(menu.x,scene.clientWidth-menuElement.offsetWidth-gutter)),y:Math.max(gutter,Math.min(menu.y,scene.clientHeight-menuElement.offsetHeight-gutter))}}
  async function assertProtocol(){if(previewMode)return true;try{const c=await lifeCapabilities();if(c.life_protocol===4&&c.grid.w===GRID_W&&c.grid.h===GRID_H&&c.floor?.shape==='half-depth'&&c.floor.max_xy_exclusive===LIFE_FLOOR.maxXYExclusive&&c.footprint_mask)return true}catch{}msg('라이프 서버가 구버전입니다. 새 life-server를 재시작해야 인테리어를 저장할 수 있습니다.');return false}
  function normalizedDesign(design:LifeState['design']){const next:LifeState['design']=JSON.parse(JSON.stringify(design));next.objects=next.objects.filter(o=>o.category!=='dining').map(o=>{const canonical=canonicalizeFurnitureGeometry(o);return canonical.category==='window'&&(canonical.wall==='north'||canonical.wall==='west')?{...canonical,rotation:wallRotation(canonical.wall)}:canonical});return next}
  async function beginEdit(){if(life&&isOwner&&await assertProtocol()){pendingView=null;draft=normalizedDesign(life.design);editorTab='design';themePage=0;selectedId='';editing=true}}
  function chooseCategory(c:FurnitureCategory){editorTab=c;category=c;furniturePage=0;selectedId='';rotation=c==='window'?wallRotation(wall):0}
  function chooseTheme(id:string,floor:string){if(draft){draft.wallpaper=id;draft.floor=floor}}
  function cancel(){editing=false;draft=null}
  async function save(){if(!life||!draft||!await assertProtocol())return;saving=true;try{const saved=await lifeSaveDesign(life.life_id,draft);life={...saved,design:normalizedDesign(saved.design)};editing=false;draft=null;msg('인테리어를 저장했어요.')}catch(e){msg(`저장 실패: ${e}`)}finally{saving=false}}
  // 내 캐릭터는 AI 스프라이트(캐시) — 데스크톱 마스코트·초상과 동일 인물 (없으면 절차 생성 폴백)
  let sprite=$state<string|null>(null);
  $effect(()=>{let un:(()=>void)|null=null;getSprite().then(v=>{sprite=v;if(v)lifeSyncMascotImage().catch(()=>{})});listen('sprite:ready',()=>getSprite().then(v=>{sprite=v;if(v)lifeSyncMascotImage().catch(()=>{})})).then(u=>un=u);return()=>un?.()});
  // 다른 점유자도 내 캐릭터와 동일 로직의 AI 스프라이트로 — 캐시 있으면 즉시, 없으면 백그라운드 생성 요청(절차 폴백 유지)
  let occSpr=$state<Record<string,string>>({}),occVersion=$state<Record<string,string>>({});
  const occSeed=(o:{mascot_seed?:string;name:string})=>o.mascot_seed||o.name;
  $effect(()=>{const occ=life?.occupants??[];for(const o of occ){const s=occSeed(o),version=o.mascot_image_sha256??'';if(me&&o.agent_id===me.agent_id)continue;if(occVersion[o.agent_id]===version&&occSpr[o.agent_id]!==undefined)continue;occVersion[o.agent_id]=version;occSpr[o.agent_id]='';const fallback=()=>getOccupantSprite(s).then(v=>{if(v)occSpr[o.agent_id]=v;else requestOccupantSprite(s)});if(version)lifeMascotImage(o.agent_id).then(v=>{if(v)occSpr[o.agent_id]=v;else fallback()}).catch(fallback);else fallback()}});
  $effect(()=>{let un:(()=>void)|null=null;listen<string>('occupant-sprite:ready',(e)=>{const s=e.payload;getOccupantSprite(s).then(v=>{if(v){const next={...occSpr};for(const o of life?.occupants??[])if(occSeed(o)===s)next[o.agent_id]=v;occSpr=next}})}).then(u=>un=u);return()=>un?.()});
  const specs=new Map<string,RobotSpec>(); function robot(node:HTMLCanvasElement,name:string){let raf=0,lastFrame=-Infinity,visible=true;const observer=new IntersectionObserver(([entry])=>{visible=entry?.isIntersecting??false});observer.observe(node);const run=(spec:RobotSpec)=>{const c=node.getContext('2d')!;const loop=(t:number)=>{if(visible&&!scrolling&&document.visibilityState==='visible'&&t-lastFrame>=66){drawRobot(c,spec,frameAt('idle',t));lastFrame=t}raf=requestAnimationFrame(loop)};loop(0)};specs.has(name)?run(specs.get(name)!):robotSpecForSeed(name).then(s=>{specs.set(name,s);run(s)});return{destroy:()=>{observer.disconnect();cancelAnimationFrame(raf)}}}
  function spriteRotation(o:LifeObject|PlacedFurniture):Rotation{return o.category==='window'&&(o.wall==='north'||o.wall==='west')?wallRotation(o.wall):o.rotation}
  function objectLayout(o:LifeObject){
    const item=FURNITURE_BY_ID.get(o.asset_id),visualRotation=spriteRotation(o),imageAnchor=item?.render.anchors[visualRotation]??[.5,1] as [number,number];
    if(o.category==='window'){const side=o.wall??'north',i=o.cell[0]+o.size[0]/2,p=side==='west'?iso(0,i):iso(i,0),width=wallSpanScreenWidth(o.size[0],TW);return{x:p[0],y:p[1]-WALL*.3,z:5,w:width,anchor:imageAnchor}}
    const [w,h]=rotatedSize(o.size,o.rotation),ground=spriteGroundAnchor(o,item?.render.footprintAnchor),p=iso(ground[0],ground[1]),width=(item?.render.widthTiles[visualRotation]??(w+h)/2)*TW;
    return{x:p[0],y:p[1],z:20+Math.round((ground[0]+ground[1])*10),w:width,anchor:imageAnchor};
  }
</script>

{#if life&&me&&activeDesign}
<div class="lifewrap" bind:this={root}>{#if flash}<div class="title"><em>{flash}</em></div>{/if}
  <div class="scene" class:moving bind:this={scene}>
    {#if isOwner&&!editing}<button class="edit-trigger" aria-label="인테리어 설정" title="인테리어 설정" onclick={beginEdit}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.09a2 2 0 0 1 1 1.74v.5a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.38a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg></button>{/if}
    <svg viewBox={`${VIEW_X} ${VIEW_Y} ${VIEW_W} ${VIEW_H}`} aria-label="아이소메트릭 미니라이프">
      <polygon class="north" fill={theme.wall} points={points([[OX,VIEW_Y],[VIEW_X+VIEW_W,VIEW_Y],b,a])}/>
      <polygon class="west" fill={theme.wallSide} points={points([[VIEW_X,VIEW_Y],[OX,VIEW_Y],a,d])}/>
      <polygon fill={theme.floorBase} points={points([a,b,d])}/>
      {#each Array(GRID_H+1) as _,y}{#each Array(GRID_W+1) as _,x}
        {#if isFloorCell([x,y])}
          <polygon class="tile" class:taken={occupied.has(`${x},${y}`)} fill={(x+y)%2?theme.floorAlt:theme.floorBase} stroke={theme.grout} points={tile(x,y)} onmouseenter={()=>hoverCell=[x,y]} onclick={()=>clickFloor(x,y)}/>
        {:else if x+y===LIFE_FLOOR.maxXYExclusive}
          <polygon class="tile floor-cut" fill={(x+y)%2?theme.floorAlt:theme.floorBase} stroke={theme.grout} points={tile(x,y)}/>
        {/if}
      {/each}{/each}
      <polyline class="room-edge" points={points([d,a,b])}/>
      <line class="room-edge" x1={OX} y1={VIEW_Y} x2={a[0]} y2={a[1]}/>
      {#if ghost&&ghost.category!=='window'}
        {#each rotatedOffsets(ghost) as offset}
          <polygon class="ghost-cell" class:blocked={!canPlace(ghost)} points={tile(ghost.cell[0]+offset[0],ghost.cell[1]+offset[1])}/>
        {/each}
      {/if}
      {#if ghost&&ghost.category==='window'&&ghost.wall}
        {#each wallOccupiedIndices(ghost.cell[0],ghost.size[0]) as wallIndex}
          <polygon class="ghost-wall-cell" class:blocked={!canPlace(ghost)} points={wallTile(ghost.wall,wallIndex)}/>
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
      {#if editing&&editorTab==='window'}
        {#each Array(GRID_W) as _,x}<polygon class="wallslot" class:hovered={hoverWall?.side==='north'&&hoverWall.index===x} points={wallTile('north',x)} onmouseenter={()=>hoverWindow(x,'north')} onclick={()=>placeWindow(x,'north')}/>{/each}
        {#each Array(GRID_H) as _,y}<polygon class="wallslot" class:hovered={hoverWall?.side==='west'&&hoverWall.index===y} points={wallTile('west',y)} onmouseenter={()=>hoverWindow(y,'west')} onclick={()=>placeWindow(y,'west')}/>{/each}
      {/if}
    </svg>
    {#if ghost}{@const gp=objectLayout(ghost)}<div class="furniture ghost" class:invalid={!canPlace(ghost)} style:left={`${(gp.x-VIEW_X)/VIEW_W*100}%`} style:top={`${(gp.y-VIEW_Y)/VIEW_H*100}%`} style:width={`${gp.w/VIEW_W*100}%`} style:transform={`translate(${-gp.anchor[0]*100}%,${-gp.anchor[1]*100}%)`} style:z-index={gp.z}><FurnitureSprite assetId={ghost.asset_id} rotation={spriteRotation(ghost)} life/></div>{/if}
    {#each activeDesign.objects as o,i (`${o.asset_id}-${i}`)}{@const p=objectLayout(o)}<button class="furniture placed" data-category={o.category} data-object-index={i} style:left={`${(p.x-VIEW_X)/VIEW_W*100}%`} style:top={`${(p.y-VIEW_Y)/VIEW_H*100}%`} style:width={`${p.w/VIEW_W*100}%`} style:transform={`translate(${-p.anchor[0]*100}%,${-p.anchor[1]*100}%)`} style:z-index={p.z} oncontextmenu={e=>openMenu(e,i)}><FurnitureSprite assetId={o.asset_id} rotation={spriteRotation(o)} label={FURNITURE_BY_ID.get(o.asset_id)?.name} life/></button>{/each}
    {#each life.occupants as o (o.agent_id)}{@const p=iso(o.cell[0]+.5,o.cell[1]+.5)}<div class="agent" style:left={`${(p[0]-VIEW_X)/VIEW_W*100}%`} style:top={`${(p[1]-VIEW_Y)/VIEW_H*100}%`} style:z-index={30+Math.round((o.cell[0]+o.cell[1])*10)}>{#if o.bubble}<i class="agent-bubble">{o.bubble}</i>{/if}{#if me&&o.agent_id===me.agent_id&&sprite}<img class="spr" src={'data:image/png;base64,'+sprite} alt={o.name} draggable="false"/>{:else if occSpr[o.agent_id]}<img class="spr" src={'data:image/png;base64,'+occSpr[o.agent_id]} alt={o.name} draggable="false"/>{:else}<canvas width="128" height="128" use:robot={o.mascot_seed||o.name}></canvas>{/if}<span>{#if editing}{o.name}{:else}<NameChip agentId={o.agent_id} name={o.name} meId={me.agent_id} myLifeId={me.my_life_id} currentLifeId={me.life_id}/>{/if}</span></div>{/each}
    {#if menu}{@const menuObject=activeDesign.objects[menu.index]}<button class="menu-dismiss" aria-label="메뉴 닫기" onclick={()=>menu=null}></button><div class="object-menu" bind:this={menuElement} style:left={`${menu.x}px`} style:top={`${menu.y}px`}><button class="menu-close" aria-label="가구 메뉴 닫기" title="닫기" onclick={()=>menu=null}></button>{#if menuObject?.category!=='window'}<button onclick={()=>rotatePlaced(menu!.index,1)}>정방향 90° 회전</button><button onclick={()=>rotatePlaced(menu!.index,-1)}>역방향 90° 회전</button>{/if}<button class="remove" onclick={()=>removePlaced(menu!.index)}>가구 배치 해제</button></div>{/if}
  </div>
  {#if editing&&draft}<section class="editor"><header><div><b>인테리어 설정</b></div><nav><button onclick={cancel}>취소</button><button class="primary" onclick={save} disabled={saving}>{saving?'저장 중…':'저장'}</button></nav></header>
    <div class="tabs"><button class:active={editorTab==='design'} onclick={()=>{editorTab='design';themePage=0;selectedId=''}}>디자인</button>{#each categories as c}<button class:active={editorTab===c} onclick={()=>chooseCategory(c)}>{CATEGORY_LABELS[c]}</button>{/each}</div>
    {#if editorTab==='design'}
      <div class="themes">{#each themeItems as t}<button class:active={draft.wallpaper===t.wallpaper} onclick={()=>chooseTheme(t.wallpaper,t.floor)}><span><i style:background={t.wall}></i><i style:background={t.floorBase}></i></span><small>{t.name}</small></button>{/each}</div>
      <div class="pager design-pager"><button aria-label="이전 디자인 페이지" onclick={()=>themePage=Math.max(0,themePage-1)} disabled={themePage===0}>‹</button><span>{themePage+1} / {themePageCount}</span><button aria-label="다음 디자인 페이지" onclick={()=>themePage=Math.min(themePageCount-1,themePage+1)} disabled={themePage===themePageCount-1}>›</button></div>
    {:else}
      <div class="catalog furniture-page">{#each furnitureItems as item}<button class:active={selectedId===item.id} onclick={()=>selectedId=item.id}><span><FurnitureSprite assetId={item.id} rotation={rotation} label={item.name}/></span><small>{item.name}</small></button>{/each}</div>
      <div class="catalog-controls">
        <div class="directions">{#if category==='window'}<span>창문 방향은 벽에 고정됩니다.</span>{:else}{#each ROTATIONS as r}<button class:active={rotation===r} onclick={()=>rotation=r}>{['↗','↘','↙','↖'][ROTATIONS.indexOf(r)]}</button>{/each}{/if}</div>
        <div class="pager"><button aria-label="이전 가구 페이지" onclick={()=>furniturePage=Math.max(0,furniturePage-1)} disabled={furniturePage===0}>‹</button><span>{furniturePage+1} / {furniturePageCount}</span><button aria-label="다음 가구 페이지" onclick={()=>furniturePage=Math.min(furniturePageCount-1,furniturePage+1)} disabled={furniturePage===furniturePageCount-1}>›</button></div>
      </div>
    {/if}
  </section>{/if}
</div>
{:else if error}<div class="empty">방 서버에 연결하지 못했습니다. 설정에서 Life Server 연결을 확인하세요.</div>{/if}

<style>
  .lifewrap{display:flex;flex-direction:column;gap:8px}.title{display:flex;align-items:center;gap:8px;font-size:12px}.title small{color:var(--ink-soft)}.title em{font-style:normal;color:var(--accent)}button{font:inherit;color:inherit}
  .scene{position:relative;width:min(100%,640px);aspect-ratio:560/257;margin:auto;overflow:hidden;contain:layout paint;isolation:isolate;transform:translateZ(0);background:linear-gradient(#fbf8ee,#e9e4d8)}.edit-trigger{position:absolute;left:10px;top:10px;z-index:1100;width:34px;height:34px;display:grid;place-items:center;padding:0;border:1px solid var(--line);border-radius:50%;background:var(--frame-bg);color:var(--ink-soft);box-shadow:var(--shadow-soft);cursor:pointer}.edit-trigger:hover{color:var(--accent-strong);border-color:var(--accent)}.edit-trigger svg{position:static;width:18px;height:18px;fill:none;stroke:currentColor;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}.scene>svg{position:absolute;inset:0;width:100%;height:100%}.room-edge{fill:none;stroke:#a69483;stroke-width:1;vector-effect:non-scaling-stroke}.tile{cursor:pointer;stroke-width:.65;transition:filter .1s}.tile:hover{filter:brightness(1.12)}.tile.taken{cursor:not-allowed}.tile.floor-cut{pointer-events:none;cursor:default}.scene.moving .tile{cursor:progress}.ghost-cell,.ghost-wall-cell{fill:rgba(68,211,159,.38);stroke:#24a77a;stroke-width:1.2;pointer-events:none}.ghost-cell.blocked,.ghost-wall-cell.blocked{fill:rgba(226,80,80,.4);stroke:#c93636}.placed-cell{fill:rgba(44,166,224,.08);stroke:#2ca6e0;stroke-width:.85;pointer-events:none}.ground-point{fill:#ff5b5b;stroke:white;stroke-width:.8;pointer-events:none}.wallslot{fill:transparent;stroke:transparent;stroke-width:.7;cursor:crosshair;transition:fill .1s}.wallslot:hover,.wallslot.hovered{fill:rgba(116,225,197,.16);stroke:#58cdb0;stroke-width:1}
  .furniture{position:absolute;height:auto;min-width:0!important;max-width:none;overflow:visible;border:0;padding:0;background:transparent;pointer-events:none}.furniture.placed{pointer-events:auto;cursor:default}.ghost{opacity:.48}.ghost.invalid{opacity:.25;filter:grayscale(1) drop-shadow(0 0 3px #d33)}.agent{position:absolute;width:11.43%;height:24.9%;transform:translate(-50%,-82%);pointer-events:none}.agent canvas{width:100%;height:100%;image-rendering:pixelated}.agent .spr{width:100%;height:100%;object-fit:contain}.agent span{position:absolute;left:50%;bottom:-3px;transform:translateX(-50%);white-space:nowrap;color:#2b2520;background:#fffaf0;border:1px solid #c8bea9;border-radius:99px;box-shadow:0 1px 3px #342d2433;padding:1px 6px;font-size:9px;font-weight:600}.object-menu{position:absolute;z-index:1001;display:flex;flex-direction:column;min-width:145px;padding:5px;color:#443b35;background:#fffdf8;border:1px solid #c8bea9;border-radius:8px;box-shadow:0 6px 20px #342d2440}.object-menu button{border:0;padding:7px 9px;text-align:left;background:transparent;border-radius:5px;cursor:pointer}.object-menu button:hover{background:var(--pastel-lavender)}.object-menu .menu-close{position:absolute;top:-10px;right:-10px;width:21px;height:21px;padding:0;border:1px solid #c8bea9;border-radius:50%;background:inherit;color:var(--danger)}.object-menu .menu-close::before,.object-menu .menu-close::after{content:'';position:absolute;left:50%;top:50%;width:8px;height:1.25px;border-radius:99px;background:currentColor;transform-origin:center}.object-menu .menu-close::before{transform:translate(-50%,-50%) rotate(45deg)}.object-menu .menu-close::after{transform:translate(-50%,-50%) rotate(-45deg)}.object-menu .menu-close:hover{background:var(--coral);color:var(--coral-ink)}.object-menu .remove{color:#b74444}.menu-dismiss{position:absolute;inset:0;z-index:1000;border:0;background:transparent}
  .agent-bubble{position:absolute;left:50%;bottom:94%;transform:translateX(-50%);box-sizing:border-box;width:max-content;min-width:76px;max-width:160px;padding:6px 10px;background:#fffdf8;color:#2b2520;border:1.5px solid #6f665b;border-radius:11px;font:600 11px/1.35 'Segoe UI','Malgun Gothic',sans-serif;text-align:center;white-space:pre-wrap;overflow-wrap:anywhere;word-break:keep-all;box-shadow:0 2px 6px #342d2438}.agent-bubble:after{content:'';position:absolute;left:50%;bottom:-5px;width:8px;height:8px;background:#fffdf8;border-right:1.5px solid #6f665b;border-bottom:1.5px solid #6f665b;transform:translateX(-50%) rotate(45deg)}
  .editor{display:flex;flex-direction:column;gap:9px;padding:12px 12px 6px;background:var(--frame-bg);border-radius:12px;box-shadow:var(--shadow-soft)}header{display:flex;align-items:center;justify-content:space-between;gap:10px}header div{display:flex;flex-direction:column}header small{color:var(--ink-soft)}nav{display:flex;align-items:center;gap:6px}nav button,.tabs button,.directions button,.pager button{border:0;border-radius:7px;padding:5px 9px;background:var(--pastel-lavender);cursor:pointer}nav button,.tabs button{font-size:13px}.primary,.tabs button.active,.directions button.active{background:var(--accent)!important;color:white}.themes,.catalog{display:grid;grid-template-columns:repeat(5,1fr);gap:6px}.catalog.furniture-page{grid-template-columns:repeat(4,1fr)}.themes button,.catalog button{height:104px;min-width:0;display:grid;grid-template-rows:80px auto;border:2px solid transparent;border-radius:8px;background:#fffdfa;overflow:hidden}.themes button.active,.catalog button.active{border-color:var(--accent)}.themes button>span{display:grid;grid-template-rows:1fr 1fr;min-width:0;min-height:0}.themes i{display:block}.themes small,.catalog small{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.tabs{display:flex;flex-wrap:wrap;gap:5px}.catalog span{display:block;min-width:0;min-height:0;overflow:hidden;padding:5px}.catalog span :global(canvas){display:block!important;width:100%!important;height:100%!important;min-width:0!important;max-width:100%!important}.catalog-controls{position:relative;display:flex;align-items:center;gap:12px;height:25px;margin-top:-4px}.pager{display:flex;align-items:center;justify-content:center;gap:8px;height:25px;font-size:11px}.design-pager{align-self:center;margin-top:-4px}.catalog-controls .pager{position:absolute;left:50%;top:50%;transform:translate(-50%,-50%)}.pager button{width:28px;height:25px;display:grid;place-items:center;padding:0;line-height:1}.pager>span{height:25px;display:grid;place-items:center;line-height:1}.pager button:disabled{opacity:.4;cursor:default}.directions{display:flex;align-items:center;gap:5px;height:25px;min-width:0;font-size:11px}.directions button{width:25px;height:25px;display:grid;place-items:center;padding:0;line-height:1}.directions span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.empty{padding:14px;background:var(--frame-bg);border-radius:12px;color:var(--ink-soft)}
</style>
