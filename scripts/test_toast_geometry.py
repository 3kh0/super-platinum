"""Pure geometry corruption tests; these samples are not native evidence."""
import unittest
from toast_geometry import validate_toast

def sample_toast(text,narrow=False):
    if narrow:
        return dict(ok=True,text=text,positioned=True,visible=True,viewport=[668,380],bounds=[230,211,650,265],
            textBounds=[242,221,590,253],dismissBounds=[610,223,634,247],
            textHit=dict(point=[416,237],hit=True),dismissHit=dict(point=[622,235],hit=True),gap=8,
            obstacles=[dict(kind='dialog',rect=[24,30,644,194]),dict(kind='composer',rect=[160,280,650,370])],focus=None)
    return dict(ok=True,text=text,positioned=True,visible=True,viewport=[1280,780],bounds=[900,600,1262,660],
        textBounds=[912,610,1210,650],dismissBounds=[1220,618,1244,642],
        textHit=dict(point=[1061,630],hit=True),dismissHit=dict(point=[1232,630],hit=True),gap=8,
        obstacles=[dict(kind='dialog',rect=[200,62.4,1000,226.4]),dict(kind='composer',rect=[300,700,1262,770])],focus=None)

class ToastGeometry(unittest.TestCase):
    def test_visible_gap_proof(self):
        for narrow in (False,True):
            proof=sample_toast('Saved',narrow);validate_toast(proof,proof['viewport'],'Saved')
    def test_hidden_missing_and_wrong_text(self):
        for key,value in [('visible',False),('positioned',False),('text','wrong'),('bounds',None),('gap',0),('dismissHit',dict(point=[1232,630],hit=False))]:
            p=sample_toast('Saved');p[key]=value
            with self.assertRaises(RuntimeError):validate_toast(p,[1280,780],'Saved')
        with self.assertRaises(RuntimeError):validate_toast(None,[1280,780],'Saved')
    def test_overlap_or_missing_protected_surface(self):
        for kind in ('composer','dialog','completion'):
            p=sample_toast('Saved');p['obstacles'].append(dict(kind=kind,rect=[910,610,980,650]))
            with self.assertRaises(RuntimeError):validate_toast(p,[1280,780],'Saved')
        p=sample_toast('Saved');p['obstacles']=p['obstacles'][:1]
        with self.assertRaises(RuntimeError):validate_toast(p,[1280,780],'Saved')
    def test_focus_clipping_nonfinite_and_forged_hit(self):
        for key,value in [('focus',[950,620,1000,640]),('bounds',[900,600,1281,660]),('textBounds',[899,610,1210,650]),('dismissBounds',[1220,618,1244,float('nan')]),('dismissHit',dict(point=[1233,630],hit=True))]:
            p=sample_toast('Saved');p[key]=value
            with self.assertRaises(RuntimeError):validate_toast(p,[1280,780],'Saved')

if __name__=='__main__':unittest.main()
