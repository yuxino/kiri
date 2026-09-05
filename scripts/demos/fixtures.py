"""Original, harmless documentation fixtures, generated only in the capture workspace."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor

ROOT = Path(__file__).resolve().parent
P = ROOT / 'fixtures'
P.mkdir(exist_ok=True)
FONT = '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'
BOLD = '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'
def font(size, bold=False):
    return ImageFont.truetype(BOLD if bold else FONT, size)

im = Image.new('RGB', (1000, 650), '#f7f8fa')
d = ImageDraw.Draw(im)
d.rounded_rectangle((35,30,965,620),18,fill='white',outline='#e0e3e9',width=2)
d.text((74,62),'A little room for good ideas',font=font(29,True),fill='#272b3b')
d.text((75,107),'SAMPLE WORKSPACE / WEEKEND NOTES',font=font(12),fill='#7c8191')
for x,y,title,body in [(75,168,'Read a few pages','One chapter, no hurry.'),(516,168,'Make something small','A tiny tool that feels useful.'),(75,369,'Take a long walk','Let the next idea find you.'),(516,369,'Leave room to play','Not everything needs a plan.')]:
    d.rounded_rectangle((x,y,x+406,y+158),12,fill='#f5f3fb',outline='#e6e0f3')
    d.ellipse((x+22,y+22,x+35,y+35),fill='#a28cdb')
    d.text((x+22,y+57),title,font=font(21,True),fill='#3c3555')
    d.text((x+22,y+98),body,font=font(16),fill='#797188')
d.text((75,572),'Original demo fixture - no personal data',font=font(12),fill='#9699a2')
im.save(P/'source.png')

pages = [('SMALL MOMENTS','A local reading sample',['A quiet morning.','A fresh page.','A small idea worth keeping.']),('SLOW DOWN','Page two',['Take a breath.','Read at your own pace.','There is no finish line.']),('MAKE ROOM','Page three',['For curious questions.','For ordinary little things.','For another good day.'])]
for i,(title,subtitle,items) in enumerate(pages,1):
    im=Image.new('RGB',(760,1060),'#fffefa');d=ImageDraw.Draw(im)
    d.text((62,60),f'LOCAL SAMPLE / {i:02}',font=font(14),fill='#8b8395')
    d.text((60,122),title,font=font(44,True),fill='#353043')
    d.text((63,189),subtitle,font=font(19),fill='#8b8395')
    for j,text in enumerate(items):
        y=260+j*224
        d.rounded_rectangle((60,y,700,y+182),18,fill=['#f0edf8','#edf4f3','#f9efe8'][j],outline='#dfdae6',width=2)
        d.text((92,y+44),f'{j+1:02}',font=font(16),fill='#9284a8')
        d.text((92,y+87),text,font=font(23,True),fill='#4d4659')
    d.text((62,998),'Original page created for the interface demo',font=font(13),fill='#91899b')
    im.save(P/f'page-{i}.png')

pdf = canvas.Canvas(str(P/'sample.pdf'),pagesize=(595,842))
pdf.setTitle('A small guide to local-first tools')
sections = [('Start with one useful thing','Notes for a quieter digital workspace',[('Keep the first step small','Pick a tool that solves one everyday problem.'),('Make it easy to understand','Let the interface explain itself through use.'),('Keep your work close','Use ordinary files and clear local boundaries.')]),('A page worth returning to','Reading is easier with a little structure',[('Find your place','An outline makes a long document feel navigable.'),('Look at the details','Zoom in, turn the page, and take your time.'),('Build your own rhythm','A small reading habit is still a real habit.')]),('Leave room for the next idea','A simple ending, not a final answer',[('Notice what helps','Keep the features that make the day easier.'),('Remove what gets in the way','A quiet workspace does not need much decoration.'),('Start again tomorrow','One useful thing is enough for today.')])]
for i,(title,subtitle,parts) in enumerate(sections,1):
    pdf.setFillColor(HexColor('#fffefa'));pdf.rect(0,0,595,842,fill=1,stroke=0)
    pdf.bookmarkPage(f'p{i}');pdf.addOutlineEntry(title,f'p{i}',0)
    pdf.setFillColor(HexColor('#8b789f'));pdf.setFont('Helvetica',10);pdf.drawString(48,785,f'LOCAL LEARNING NOTES / {i:02}')
    pdf.setFillColor(HexColor('#343044'));pdf.setFont('Helvetica-Bold',25);pdf.drawString(48,723,title)
    pdf.setFont('Helvetica',12);pdf.setFillColor(HexColor('#827b8c'));pdf.drawString(48,691,subtitle)
    for j,(heading,body) in enumerate(parts):
        y=555-j*147;pdf.setFillColor(HexColor('#f3f0f8'));pdf.roundRect(48,y-25,499,107,10,fill=1,stroke=0)
        pdf.setFillColor(HexColor('#554a69'));pdf.setFont('Helvetica-Bold',15);pdf.drawString(67,y+45,f'{j+1:02}  {heading}')
        pdf.setFont('Helvetica',11);pdf.setFillColor(HexColor('#7a7384'));pdf.drawString(67,y+13,body)
    pdf.setFillColor(HexColor('#9b95a4'));pdf.setFont('Helvetica',9)
    pdf.drawString(48,44,'Original demo document. No personal files or AI-generated answers.')
    pdf.drawRightString(545,44,str(i));pdf.showPage()
pdf.save()
