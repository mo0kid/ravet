-- Public synthetic bus check; run only in an empty disposable test resource.
local root=reaper.GetResourcePath()..'/ravet-validation'
local log=assert(io.open(root..'/sidechain-host.log','w'))
local function note(s) log:write(s..'\n');log:flush() end
local function check(ok,s) assert(ok,s);note('PASS '..s) end
local track,fx
local function guard(fn)
  local ok,err=xpcall(fn,debug.traceback)
  if not ok then reaper.OnStopButton();note('FAIL '..err);log:close() end
end
guard(function()
  note('REAPER '..reaper.GetAppVersion()..' '..reaper.GetOS())
  check(reaper.file_exists(root..'/sidechain-input.wav'),'explicit synthetic four-channel source exists')
  if reaper.CountTracks(0)==1 then
    local prior=reaper.GetTrack(0,0)
    local _,name=reaper.GetSetMediaTrackInfo_String(prior,'P_NAME','',false)
    check(name=='Public synthetic sidechain test','rerun owns only its named synthetic track')
    reaper.OnStopButton();reaper.DeleteTrack(prior)
  end
  check(reaper.CountTracks(0)==0,'empty disposable test project')
  reaper.OnStopButton()
  local master=reaper.GetMasterTrack(0)
  reaper.SetMediaTrackInfo_Value(master,'D_VOL',0)
  for i=0,reaper.GetTrackNumSends(master,1)-1 do reaper.SetTrackSendInfo_Value(master,1,i,'D_VOL',0) end
  reaper.InsertTrackAtIndex(0,true);track=reaper.GetTrack(0,0)
  reaper.GetSetMediaTrackInfo_String(track,'P_NAME','Public synthetic sidechain test',true)
  reaper.SetMediaTrackInfo_Value(track,'I_NCHAN',4)
  reaper.SetOnlyTrackSelected(track)
  check(reaper.InsertMedia(root..'/sidechain-input.wav',0)==1,'synthetic source inserted')
  local item=reaper.GetTrackMediaItem(track,0)
  check(item~=nil,'source item present')
  reaper.SetMediaItemInfo_Value(item,'D_FADEINLEN',0);reaper.SetMediaItemInfo_Value(item,'D_FADEOUTLEN',0)
  fx=reaper.TrackFX_AddByName(track,'CLAP: RAVET Sidechain (RAVET)',false,-1)
  check(fx>=0,'native sidechain reference discovered')
  local _,ins,outs=reaper.TrackFX_GetIOSize(track,fx)
  check(ins==3 and outs==2,'stereo main plus mono input-only detector pins')
  for pin=0,2 do check(reaper.TrackFX_SetPinMappings(track,fx,0,pin,1<<pin,0),'independent input pin '..pin) end
  for pin=0,1 do check(reaper.TrackFX_SetPinMappings(track,fx,1,pin,1<<pin,0),'main output pin '..pin) end
  local lo,hi=reaper.TrackFX_GetPinMappings(track,fx,0,2)
  check(lo==4 and hi==0,'detector receives only channel three')
  check(reaper.GetSetProjectInfo_String(0,'RECORD_PATH',root,true),'explicit generated media directory')
  reaper.Main_SaveProjectEx(0,root..'/sidechain-source.rpp',8)
  reaper.SetMediaItemSelected(item,true)
  local section=reaper.SectionFromUniqueID(0)
  check(reaper.kbd_getTextFromCmd(40209,section)=='Item: Apply track/take FX to items','verified stereo apply-FX action')
  local original=reaper.GetActiveTake(item)
  reaper.Main_OnCommand(40209,0)
  local rendered=reaper.GetActiveTake(item)
  check(rendered~=nil and rendered~=original,'native DAW creates processed stereo take')
  local source=reaper.GetMediaItemTake_Source(rendered)
  note('rendered='..reaper.GetMediaSourceFileName(source,''))
  local channels=reaper.GetMediaSourceNumChannels(source)
  check(channels>=2 and channels<=4,'bounded rendered channels')
  note('rendered channels='..channels)
  reaper.TrackFX_SetEnabled(track,fx,false) -- rendered source already includes the gate
  reaper.Main_SaveProjectEx(0,root..'/sidechain-rendered.rpp',8)
  for i=0,reaper.GetTrackNumSends(master,1)-1 do check(reaper.GetTrackSendInfo_Value(master,1,i,'D_VOL')==0,'hardware send silent') end
  check(reaper.GetPlayState()==0,'test transport remains stopped')
  note('PASS REAPER sidechain native routing/render prepared; check file samples separately');log:close()
end)
