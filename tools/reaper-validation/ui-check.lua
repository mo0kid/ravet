-- Run after validate.lua, only in its disposable REAPER configuration.
-- Leave a silent looping GUI interaction test; observe edits and recorded automation.
local root = reaper.GetResourcePath() .. '/ravet-validation'
local log = assert(io.open(root .. '/ui-validation.log', 'w'))
local function note(s) log:write(s .. '\n'); log:flush() end
local function check(ok,s) assert(ok,s); note('PASS ' .. s) end
local function guard(fn)
  local ok,err=xpcall(fn,debug.traceback)
  if not ok then note('FAIL ' .. err); reaper.OnStopButton(); log:close() end
  return ok
end
local track,env
local changed=false
local deadline,previous
local function monitor()
  if not guard(function()
    local gain=reaper.TrackFX_GetParam(track,0,0)
    local bypass=reaper.TrackFX_GetParam(track,0,1)
    if gain~=previous then note('GUI host gain=' .. gain); previous=gain end
    if math.abs(gain-1.5)>0.01 then changed=true end
    if changed and bypass==1 then
      if not deadline then deadline=reaper.time_precise()+0.3 end
      if reaper.time_precise()>=deadline then
        local left,right=reaper.Track_GetPeakInfo(track,0),reaper.Track_GetPeakInfo(track,1)
        note('Bypass live peaks left=' .. left .. ' right=' .. right)
        check(math.abs(left-0.25)<0.001 and math.abs(right-0.5)<0.001,'GUI bypass reaches live stereo audio')
        reaper.OnStopButton()
        check(reaper.CountEnvelopePoints(env)>1,'GUI gain writes DAW automation points')
        local changedPoint=false
        for i=0,reaper.CountEnvelopePoints(env)-1 do
          local ok,_,value=reaper.GetEnvelopePoint(env,i)
          if ok and math.abs(value-1.5)>0.01 then changedPoint=true end
        end
        check(changedPoint,'recorded automation contains changed GUI gain')
        reaper.Main_SaveProjectEx(0,root .. '/ui-recording.rpp',8)
        note('PASS native interaction observer completed; inspect resize/layout separately')
        log:close(); return
      end
    end
    reaper.defer(monitor)
  end) then return end
end
guard(function()
  reaper.Main_openProject('noprompt:' .. root .. '/wet.rpp')
  track=reaper.GetTrack(0,0)
  local _,name=reaper.GetSetMediaTrackInfo_String(track,'P_NAME','',false)
  check(name=='RAVET Validation','disposable UI test track')
  reaper.SetMediaTrackInfo_Value(reaper.GetMasterTrack(0),'D_VOL',0)
  env=reaper.GetFXEnvelope(track,0,0,true)
  reaper.DeleteEnvelopePointRange(env,-1,10)
  reaper.InsertEnvelopePoint(env,0,1.5,1,0,false,true)
  reaper.Envelope_SortPoints(env)
  reaper.SetTrackAutomationMode(track,3) -- write GUI edits into host automation
  reaper.GetSet_LoopTimeRange(true,true,0,4,false)
  reaper.GetSetRepeat(1)
  reaper.SetEditCurPos(0,false,false)
  reaper.TrackFX_Show(track,0,2) -- hide any restored floating wrapper
  reaper.TrackFX_Show(track,0,1) -- resizable host FX chain
  reaper.OnPlayButton()
  note('READY resize editor, drag gain away from 1.5, then enable plug-in bypass')
  reaper.defer(monitor)
end)
