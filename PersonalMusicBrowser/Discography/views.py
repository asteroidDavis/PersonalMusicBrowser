from django.db.models import ForeignKey
from django.shortcuts import render
from django.views.generic import CreateView,DeleteView, ListView, UpdateView

from PersonalMusicBrowser.Discography.models import Song

# Create your views here.


class SongList(ListView):
    model = Song


class CreateSong(CreateView):
    model = Song
    fields = ['title', 'album', 'artists', 'sheet_music', 'lyrics']
