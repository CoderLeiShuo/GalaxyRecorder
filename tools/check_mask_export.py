"""Decode real H.264 exports and check the privacy block, including the right edge."""
import json, math, pathlib, subprocess, tempfile
width,height=1280,720
region={'x':.62,'y':0,'width':.38,'height':.09}
with tempfile.TemporaryDirectory(prefix='jixing-mask-') as directory:
    source=pathlib.Path(directory)/'source.mp4';out=pathlib.Path(directory)/'masked.mp4'
    def run(*args):return subprocess.run(['ffmpeg','-hide_banner','-loglevel','error',*map(str,args)],check=True,stdout=subprocess.PIPE).stdout
    run('-f','lavfi','-i',f'color=c=white:s={width}x{height}:r=10','-t','2','-c:v','libx264','-y',source)
    mask=f"drawbox=x=floor(iw*{region['x']}):y=floor(ih*{region['y']}):w=ceil(iw*{region['width']})+2:h=ceil(ih*{region['height']})+2:color=black@1:t=fill"
    run('-i',source,'-vf',mask,'-c:v','libx264','-crf','18','-y',out)
    checks=[]
    for time in [0,.9,1.8]:
        frame=run('-i',out,'-ss',time,'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-')
        assert len(frame)==width*height*3
        peak=0
        for y in range(0,math.floor(region['height']*height)-4):
            row=frame[(y*width+math.ceil(region['x']*width)+4)*3:(y*width+width)*3]
            peak=max(peak,max(row))
        assert peak<=40,(time,peak)
        assert min(frame[(height//2*width+width//2)*3:(height//2*width+width//2)*3+3])>=235
        checks.append({'time':time,'maskedPeakRgb':peak,'rightmostColumnCovered':True,'otherPicturePreserved':True})
    report={'actualH264Export':True,'androidGpuRuntimeVerified':False,'region':region,'checks':checks}
    pathlib.Path('analysis/vin-mask-0.8.1-verification.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report))
