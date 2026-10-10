-- Public synthetic validation. CLI runner must use its own prepared resource
-- directory and a new empty project; never a production resource/project.
local resource=reaper.GetResourcePath()
local root=resource..'/ravet-validation'
local marker=assert(io.open(root..'/public-fixture.txt','r'),'prepare public fixtures first')
assert(marker:read('*a')=='RAVET public synthetic validation\n','public fixture marker')
marker:close()
assert(reaper.CountTracks(0)==0,'new empty disposable project required')
local log=assert(io.open(root..'/native-meter-host.log','w'))
local function note(text) log:write(text..'\n');log:flush() end
local track,fx,deadline
local fields={'Input L peak','Input L RMS','Input L invalid','Input R peak','Input R RMS','Input R invalid','Output L peak','Output L RMS','Output L invalid','Output R peak','Output R RMS','Output R invalid'}
local parameters={}
local function check(value,text) assert(value,text);note('PASS '..text) end
local function guard(fn)
 local okay,error=xpcall(fn,debug.traceback)
 if not okay then reaper.OnStopButton();note('FAIL '..error);log:close() end
 return okay
end
local function tick()
 guard(function()
  if reaper.time_precise()<deadline then reaper.defer(tick);return end
  local expected={.0625,.0625,0.,.125,.125,0.,.125,.125,0.,.25,.25,0.}
  for i,name in ipairs(fields) do
   local value=reaper.TrackFX_GetParamNormalized(track,fx,parameters[name])
   note(name..' normalized='..value)
   check(math.abs(value-expected[i])<0.0002,'host output meter '..name)
  end
  check(reaper.TrackFX_GetFloatingWindow(track,fx)~=nil,'native editor floating-window handle')
  reaper.OnStopButton();reaper.TrackFX_Show(track,fx,2)
  note('PASS native VST3 REAPER meter parameter forwarding completed; no visual/assistive-tech claim')
  log:close()
 end)
end
guard(function()
 note('REAPER '..reaper.GetAppVersion()..' '..reaper.GetOS())
 local name
 local enumerated=0
 for i=0,100000 do
  local exists,display=reaper.EnumInstalledFX(i)
  if not exists then break end
  enumerated=enumerated+1
  if display:find('RAVET Gain',1,true) then note('Public gain candidate: '..display) end
  if display:find('VST3: RAVET Gain',1,true) then name=display;break end
 end
 note('Installed FX enumeration count='..enumerated)
 check(name~=nil,'RAVET VST3 discovery');note(name)
 local master=reaper.GetMasterTrack(0)
 reaper.SetMediaTrackInfo_Value(master,'D_VOL',1)
 for send=0,reaper.GetTrackNumSends(master,1)-1 do
  check(reaper.SetTrackSendInfo_Value(master,1,send,'D_VOL',0),'mute test hardware destination')
 end
 reaper.InsertTrackAtIndex(0,true);track=reaper.GetTrack(0,0);reaper.SetOnlyTrackSelected(track)
 reaper.InsertMedia(root..'/input.wav',0)
 fx=reaper.TrackFX_AddByName(track,name,false,1);check(fx>=0,'actual native editor instance')
 local gain,bypass
 for i=0,reaper.TrackFX_GetNumParams(track,fx)-1 do
  local _,name=reaper.TrackFX_GetParamName(track,fx,i)
  local _,id=reaper.TrackFX_GetParamIdent(track,fx,i)
  note('Parameter '..i..' '..name..' id='..id)
  parameters[name]=i
  if name=='Gain' then gain=i elseif name=='Bypass' and id~='bypass' then bypass=i end
 end
 check(gain~=nil and bypass~=nil,'gain/bypass IDs')
 for _,name in ipairs(fields) do check(parameters[name]~=nil,'meter ID '..name) end
 reaper.TrackFX_SetParamNormalized(track,fx,gain,.5)
 reaper.TrackFX_SetParamNormalized(track,fx,bypass,0)
 reaper.TrackFX_Show(track,fx,3)
 reaper.SetEditCurPos(.5,false,false)
 reaper.GetSet_LoopTimeRange(true,false,0,4,false);reaper.GetSetRepeat(1)
 reaper.OnPlayButton();deadline=reaper.time_precise()+1.5;reaper.defer(tick)
end)
